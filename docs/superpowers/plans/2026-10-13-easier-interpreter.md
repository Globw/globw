# Easier Interpreter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `easier.exe`, an interpreter for the Easier pseudocode language that type-checks programs before running them and executes them on a fast register bytecode VM.

**Architecture:** A classic pipeline of five phases, one Rust module each. The lexer produces tokens and the parser produces the AST. The checker resolves names, infers types with a monotone fixpoint (functions are specialized per argument types), checks definite assignment and return paths, and emits a typed IR. The compiler lowers the typed IR to type-specialized register bytecode, and the VM runs it. `lib.rs` exposes `compile` and `run`, and `main.rs` is the CLI.

**Tech Stack:** Rust 1.99 stable (edition 2021), Cargo, no external crates. PowerShell for scripts and Python 3.13 for benchmark comparison.

**Spec:** `docs/spec.md`

## Global Constraints

- Rust edition 2021 and **no dependencies** in `Cargo.toml`.
- `cargo` is not on PATH in a fresh shell. Run `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"` first in every PowerShell session.
- All user-visible text (errors, hints, CLI messages) is English and must match the exact wording given in the tasks. Tests assert it.
- **Spelling rules**:
  - Keywords are UPPERCASE only.
  - `variables:` is lowercase.
  - `input` and `output` are lowercase.
  - `LENGTH`, `APPEND` and `REMOVE` are uppercase.
- Indices start at 1 everywhere (arrays, matrices, lists, strings).
- **Error format**:
  - `Error at line L, column C: message`, then the source line indented by 4 spaces, a caret line, and an optional `Hint: ...` line.
  - Run-time errors use the same format.
- **Error reporting**:
  - The lexer and the parser stop at the first error.
  - The checker reports up to 20 errors, sorted by position.
- Exit codes: 0 ok, 1 error before running, 2 error while running, 3 usage error.
- Performance target: at least 5× faster than CPython on the three benchmarks in Task 12.
- Every commit message ends with the line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Pass it as a second `-m`.

## Review Focus

1. **Windows-authored files**: `.ez` files saved by Notepad have CRLF line endings and may start with a BOM. They must lex exactly like LF files. Test: `crlf_and_bom_are_accepted` (Task 2).
2. **Italian users write accented names and text** (`città`, `"perché"`). Identifiers must accept them, and caret markers must align by characters, not bytes. Tests: `caret_counts_characters_not_bytes` (Task 1), `accented_identifiers_and_strings` (Task 2).
3. **The classic student bug**: a variable is assigned only inside an `IF` without `ELSE`, or only inside a `WHILE`, and then used. This must be a compile-time error, not a crash or a silent garbage value. Tests: `use_before_assignment_*` (Task 6).
4. **Runaway recursion** (a wrong base case) must stop with a readable error, not a stack overflow or a hang. Test: `runaway_recursion_is_an_error` (Task 9).
5. **Typed input from pipes and consoles** arrives with trailing `\r`, surrounding spaces, or a decimal comma. It must convert cleanly, and bad input must give a clear error. Test: `read_converts_and_trims` (Task 9).

---

## File Map

| File | Responsibility |
|---|---|
| `Cargo.toml`, `.gitignore`, `.gitattributes` | project setup |
| `src/errors.rs` | `Pos`, `Error`, `Error::render` |
| `src/lexer.rs` | `Tok`, `Token`, `tokenize`, keyword helpers |
| `src/ast.rs` | syntax tree types |
| `src/parser.rs` | `parse`, expression and statement parsing |
| `src/tir.rs` | `BaseType`, `Type`, `join`, `Place`, typed IR |
| `src/checker/mod.rs` | `check`, `Checker`, `Var`, `Instance`, inference driver |
| `src/checker/exprs.rs` | expression checking and type hints |
| `src/checker/stmts.rs` | statement checking and definite assignment |
| `src/checker/calls.rs` | user function calls and specialization |
| `src/checker/tests.rs` | checker tests |
| `src/bytecode.rs` | `Op`, `Cmp`, `Function`, `Module` |
| `src/vm.rs` | `Value`, `run`, the interpreter loop |
| `src/compiler.rs` | `compile(&TProgram) -> Module` |
| `src/lib.rs` | module list, `compile`, `run` |
| `src/main.rs` | CLI |
| `tests/examples.rs`, `tests/cli.rs` | integration tests |
| `examples/*.ez`, `*.input`, `*.expected` | example programs and expected output |
| `bench/*` | benchmark programs and runner |

---

### Task 1: Project skeleton and error type

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `.gitattributes`, `src/lib.rs`, `src/errors.rs`

**Interfaces:**
- Produces: `errors::Pos { line: u32, col: u32, len: u32 }` with `Pos::new(line, col, len)` and `Default`. `col == 0` means "no column".
- Produces: `errors::Error { pos: Pos, message: String, hint: Option<String> }` with `Error::new(pos, impl Into<String>)`, `.with_hint(impl Into<String>)`, `.render(&self, source: &str) -> String`. Derives `Debug, Clone, PartialEq`.

- [ ] **Step 1: Create the project files**

`Cargo.toml`:
```toml
[package]
name = "easier"
version = "0.1.0"
edition = "2021"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
```

`.gitignore`:
```
/target
```

`.gitattributes`:
```
* text=auto eol=lf
```

`src/lib.rs`:
```rust
//! Easier: an interpreter for school-style pseudocode.

pub mod errors;
```

- [ ] **Step 2: Write the failing tests**

Create `src/errors.rs` containing only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_line_caret_and_hint() {
        let src = "variables: a\n    total <- 0\n";
        let e = Error::new(Pos::new(2, 5, 5), "variable \"total\" is not declared")
            .with_hint("add it to \"variables:\" at the top of the program");
        let expected = concat!(
            "Error at line 2, column 5: variable \"total\" is not declared\n",
            "        total <- 0\n",
            "        ^^^^^\n",
            "Hint: add it to \"variables:\" at the top of the program\n",
        );
        assert_eq!(e.render(src), expected);
    }

    #[test]
    fn renders_whole_line_errors_without_caret() {
        let e = Error::new(Pos::new(1, 0, 0), "division by zero");
        assert_eq!(e.render("output(1 / 0)"), "Error at line 1: division by zero\n    output(1 / 0)\n");
    }

    #[test]
    fn caret_counts_characters_not_bytes() {
        let e = Error::new(Pos::new(1, 5, 1), "x");
        assert_eq!(e.render("è é x"), "Error at line 1, column 5: x\n    è é x\n        ^\n");
    }

    #[test]
    fn strips_windows_line_endings() {
        let e = Error::new(Pos::new(1, 1, 1), "x");
        assert_eq!(e.render("ab\r\ncd"), "Error at line 1, column 1: x\n    ab\n    ^\n");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib errors`
Expected: compile error, `cannot find type Error` / `Pos`.

- [ ] **Step 4: Implement**

Add above the test module in `src/errors.rs`:
```rust
//! Source positions and the error type shared by every phase.

/// A position in the source: 1-based line and column (counted in characters)
/// and the length of the span to highlight. `col == 0` means "whole line".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
    pub len: u32,
}

impl Pos {
    pub fn new(line: u32, col: u32, len: u32) -> Pos {
        Pos { line, col, len }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub pos: Pos,
    pub message: String,
    pub hint: Option<String>,
}

impl Error {
    pub fn new(pos: Pos, message: impl Into<String>) -> Error {
        Error { pos, message: message.into(), hint: None }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Error {
        self.hint = Some(hint.into());
        self
    }

    /// Renders the error with the offending source line and a caret marker.
    pub fn render(&self, source: &str) -> String {
        let mut out = if self.pos.col > 0 {
            format!("Error at line {}, column {}: {}\n", self.pos.line, self.pos.col, self.message)
        } else {
            format!("Error at line {}: {}\n", self.pos.line, self.message)
        };
        let index = (self.pos.line as usize).saturating_sub(1);
        if let Some(text) = source.lines().nth(index) {
            let text = text.trim_end_matches('\r');
            out.push_str("    ");
            out.push_str(text);
            out.push('\n');
            if self.pos.col > 0 {
                let indent: String = text
                    .chars()
                    .take(self.pos.col as usize - 1)
                    .map(|c| if c == '\t' { '\t' } else { ' ' })
                    .collect();
                out.push_str("    ");
                out.push_str(&indent);
                out.push_str(&"^".repeat(self.pos.len.max(1) as usize));
                out.push('\n');
            }
        }
        if let Some(hint) = &self.hint {
            out.push_str("Hint: ");
            out.push_str(hint);
            out.push('\n');
        }
        out
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib errors`
Expected: 4 passed.

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml .gitignore .gitattributes src/lib.rs src/errors.rs
git commit -m "Add project skeleton and error type" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Lexer

**Files:**
- Create: `src/lexer.rs`
- Modify: `src/lib.rs` (add `pub mod lexer;`)

**Interfaces:**
- Consumes: `errors::{Error, Pos}`.
- Produces: `lexer::Tok` (enum below), `lexer::Token { tok: Tok, pos: Pos }`, `lexer::tokenize(&str) -> Result<Vec<Token>, Error>`.
  - Consecutive newlines collapse into one `Tok::Newline`.
  - The token list always ends with `Newline, Eof`. The newline is skipped if the last token is already a newline.
- Produces: `lexer::keyword(&str) -> Option<Tok>`, `lexer::miscased_statement_keyword(&str) -> Option<String>`, `lexer::miscased_value_keyword(&str) -> Option<String>`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod lexer;` to `src/lib.rs`, then create `src/lexer.rs` containing only:
```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib lexer`
Expected: compile error, `cannot find function tokenize`.

- [ ] **Step 3: Implement**

Add above the test module in `src/lexer.rs`:
```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib lexer`
Expected: 13 passed.

- [ ] **Step 5: Commit**

```powershell
git add src/lib.rs src/lexer.rs
git commit -m "Add lexer" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: AST and expression parser

**Files:**
- Create: `src/ast.rs`, `src/parser.rs`
- Modify: `src/lib.rs` (add `pub mod ast;` and `pub mod parser;`)

**Interfaces:**
- Consumes: `lexer::{Tok, Token}`, `errors::{Error, Pos}`.
- Produces: all `ast` types below (used by every later task).
- Produces: `parser::Parser { t: Vec<Token>, i: usize }` with `fn expr(&mut self) -> Result<Expr, Error>`, plus `parser::describe(&Tok) -> String`. In tests the parser is built directly as `Parser { t: tokenize(src).unwrap(), i: 0 }`.

- [ ] **Step 1: Write the AST**

Create `src/ast.rs`:
```rust
//! The syntax tree produced by the parser.

use crate::errors::Pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    /// `/`, always a real division
    Div,
    /// `DIV`, integer division
    IntDiv,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Real(f64),
    Str(String),
    Bool(bool),
    Var(String),
    Index(String, Vec<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnOp, Box<Expr>),
    Call(String, Vec<Expr>),
}

/// The left side of an assignment: `x`, `v[i]` or `m[i, j]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub name: String,
    pub indices: Vec<Expr>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Assign(Target, Expr),
    If { branches: Vec<(Expr, Vec<Stmt>)>, else_body: Option<Vec<Stmt>> },
    While(Expr, Vec<Stmt>),
    For { var: String, var_pos: Pos, from: Expr, to: Expr, step: Option<Expr>, body: Vec<Stmt> },
    Repeat(Vec<Stmt>, Expr),
    Call(String, Vec<Expr>),
    Return(Option<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Shape {
    Scalar,
    Array(usize),
    Matrix(usize, usize),
    List,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub name: String,
    pub shape: Shape,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subprogram {
    pub name: String,
    pub is_function: bool,
    pub params: Vec<(String, Pos)>,
    pub locals: Vec<Decl>,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub globals: Vec<Decl>,
    pub locals: Vec<Decl>,
    pub subprograms: Vec<Subprogram>,
    pub body: Vec<Stmt>,
}
```

- [ ] **Step 2: Write the failing tests**

Add `pub mod ast;` and `pub mod parser;` to `src/lib.rs`. Create `src/parser.rs` containing only:
```rust
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
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib parser`
Expected: compile error, `cannot find struct Parser`.

- [ ] **Step 4: Implement**

Add above the test module in `src/parser.rs`:
```rust
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib parser`
Expected: 5 passed. Dead-code warnings for helpers used in Task 4 are fine.

- [ ] **Step 6: Commit**

```powershell
git add src/lib.rs src/ast.rs src/parser.rs
git commit -m "Add AST and expression parser" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Statement and program parser

**Files:**
- Modify: `src/parser.rs`

**Interfaces:**
- Consumes: `lexer::{tokenize, miscased_statement_keyword}`, everything from Task 3.
- Produces: `parser::parse(Vec<Token>) -> Result<Program, Error>`.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `src/parser.rs`:
```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib parser`
Expected: compile error, `cannot find function parse`.

- [ ] **Step 3: Implement**

In `src/parser.rs`, change the lexer import to:
```rust
use crate::lexer::{miscased_statement_keyword, Tok, Token};
```
Add this function after `fn binary`:
```rust
pub fn parse(tokens: Vec<Token>) -> Result<Program, Error> {
    Parser { t: tokens, i: 0 }.program()
}
```
Add these methods inside `impl Parser`, after `expect`:
```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib parser`
Expected: 20 passed.

- [ ] **Step 5: Commit**

```powershell
git add src/parser.rs
git commit -m "Parse statements, blocks and program structure" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Typed IR

**Files:**
- Create: `src/tir.rs`
- Modify: `src/lib.rs` (add `pub mod tir;`)

**Interfaces:**
- Consumes: `ast::{BinOp, Shape}`, `errors::Pos`.
- Produces: everything below. The checker (Tasks 6–7) builds it and the compiler (Task 10) reads it.
  - `TExpr.ty` is `Type::Unknown` only while inferring or after an error has been reported.
  - `TFunction` index 0 is always the main program.

- [ ] **Step 1: Write the failing tests**

Add `pub mod tir;` to `src/lib.rs`. Create `src/tir.rs` containing only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use BaseType::*;

    #[test]
    fn join_widens_integer_to_real() {
        assert_eq!(join(Int, Int), Some(Int));
        assert_eq!(join(Int, Real), Some(Real));
        assert_eq!(join(Real, Int), Some(Real));
        assert_eq!(join(Str, Str), Some(Str));
        assert_eq!(join(Str, Int), None);
        assert_eq!(join(Bool, Real), None);
    }

    #[test]
    fn names_for_messages() {
        assert_eq!(Int.name(), "an integer");
        assert_eq!(Str.plural(), "strings");
        assert_eq!(Type::Array(None).name(), "an array");
        assert_eq!(Type::Base(Real).name(), "a real");
        assert_eq!(Type::Base(Bool).base(), Some(Bool));
        assert_eq!(Type::List(Some(Int)).base(), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib tir`
Expected: compile error, `cannot find function join`.

- [ ] **Step 3: Implement**

Add above the test module in `src/tir.rs`:
```rust
//! Typed intermediate representation: produced by the checker, consumed by the compiler.

use std::rc::Rc;

use crate::ast::{BinOp, Shape};
use crate::errors::Pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseType {
    Int,
    Real,
    Str,
    Bool,
}

impl BaseType {
    pub fn name(self) -> &'static str {
        match self {
            BaseType::Int => "an integer",
            BaseType::Real => "a real",
            BaseType::Str => "a string",
            BaseType::Bool => "a boolean",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            BaseType::Int => "integers",
            BaseType::Real => "reals",
            BaseType::Str => "strings",
            BaseType::Bool => "booleans",
        }
    }
}

/// The type of a value. For collections the payload is the element type,
/// `None` while it is still unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Unknown,
    Base(BaseType),
    Array(Option<BaseType>),
    Matrix(Option<BaseType>),
    List(Option<BaseType>),
}

impl Type {
    pub fn base(self) -> Option<BaseType> {
        match self {
            Type::Base(b) => Some(b),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Type::Unknown => "of unknown type",
            Type::Base(b) => b.name(),
            Type::Array(_) => "an array",
            Type::Matrix(_) => "a matrix",
            Type::List(_) => "a list",
        }
    }
}

/// The smallest type that holds both: equal types stay, integer and real
/// widen to real, anything else is incompatible.
pub fn join(a: BaseType, b: BaseType) -> Option<BaseType> {
    match (a, b) {
        (x, y) if x == y => Some(x),
        (BaseType::Int, BaseType::Real) | (BaseType::Real, BaseType::Int) => Some(BaseType::Real),
        _ => None,
    }
}

/// Where a variable lives: a register of the current function or a global slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place {
    Local(u16),
    Global(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TExpr {
    pub kind: TExprKind,
    pub ty: Type,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TExprKind {
    Int(i64),
    Real(f64),
    Str(Rc<str>),
    Bool(bool),
    /// the value of a variable (for collections: a reference to it)
    Load(Place),
    /// an element of an array/list (1 index) or matrix (2 indices)
    Elem(Place, Vec<TExpr>),
    /// one character of a string: `s[i]`
    Char(Box<TExpr>, Box<TExpr>),
    ToReal(Box<TExpr>),
    ToStr(Box<TExpr>),
    /// the `BaseType` is the type of both operands (after conversions)
    Binary(BinOp, BaseType, Box<TExpr>, Box<TExpr>),
    Neg(BaseType, Box<TExpr>),
    Not(Box<TExpr>),
    /// call of `TProgram::functions[index]`
    Call(usize, Vec<TExpr>),
    Length(Box<TExpr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TTarget {
    pub place: Place,
    pub indices: Vec<TExpr>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TStmt {
    Assign(TTarget, TExpr),
    If(Vec<(TExpr, Vec<TStmt>)>, Option<Vec<TStmt>>),
    While(TExpr, Vec<TStmt>),
    For { var: Place, from: TExpr, to: TExpr, step: Option<TExpr>, body: Vec<TStmt>, pos: Pos },
    Repeat(Vec<TStmt>, TExpr),
    Call(usize, Vec<TExpr>, Pos),
    Output(Vec<TExpr>),
    Input(Vec<(TTarget, BaseType)>),
    Append(TExpr, TExpr),
    Remove(TExpr, TExpr, Pos),
    Return(Option<TExpr>, Pos),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub name: String,
    pub ty: Type,
    pub shape: Shape,
    pub is_param: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TFunction {
    pub name: String,
    pub n_params: usize,
    /// parameters first, then declared locals; slot k lives in register k
    pub locals: Vec<Slot>,
    pub ret: Option<BaseType>,
    pub body: Vec<TStmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TProgram {
    pub globals: Vec<Slot>,
    /// index 0 is the main program
    pub functions: Vec<TFunction>,
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib tir`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```powershell
git add src/lib.rs src/tir.rs
git commit -m "Add typed IR" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Checker: scopes, type inference, expressions and statements

This task builds the whole checker except user-defined function calls, which arrive in Task 7. For now, a call to any user function reports "does not exist".

**How inference works:**
- **Variable state**: every variable has `ty: Option<BaseType>`. For collections this is the element type.
- **Rounds**: the checker walks the program repeatedly. Each walk is a *round*. During a round:
  - an assignment *widens* the target with the value's type (int + real → real; incompatible types → a recorded conflict);
  - an operation *hints* operands whose type is still unknown (`n > 0` makes `n` an integer).
- **Fixpoint**: types only grow, so the rounds reach a fixpoint. Anything still unknown then becomes `string`, and the rounds repeat.
- **Final round**: one last round runs with `is_final = true`. Only this round reports errors and keeps the typed IR.

**Files:**
- Create: `src/checker/mod.rs`, `src/checker/exprs.rs`, `src/checker/stmts.rs`, `src/checker/calls.rs`, `src/checker/tests.rs`
- Modify: `src/lib.rs` (add `pub mod checker;`)

**Interfaces:**
- Consumes: `ast::*`, `tir::*`, `errors::{Error, Pos}`, `lexer::{miscased_statement_keyword, miscased_value_keyword}`.
- Produces: `checker::check(&Program) -> Result<TProgram, Vec<Error>>`.
- Produces (crate-internal, used by Task 7):
  - `Checker` fields: `prog`, `globals`, `instances`, `by_key`, `sub_by_name`, `changed`, `is_final`, `names_only`, `errors`, `current`, `queue`, `visited`, `assigned`, `final_queue`.
  - `Var` and `Instance`.
  - Methods: `report`, `report_structural`, `reach`, `visit`, `instance_count`, `widen`, `hint`, `lookup`, `var`, `expr`, `unknown_function`.
  - Free functions: `exprs::convert(TExpr, Option<BaseType>) -> TExpr` and `exprs::unknown(Pos) -> TExpr`.

- [ ] **Step 1: Write the failing tests**

Add `pub mod checker;` to `src/lib.rs`. Create `src/checker/tests.rs`:
```rust
use super::*;
use crate::lexer::tokenize;
use crate::parser::parse;

pub(super) fn check_src(src: &str) -> Result<TProgram, Vec<Error>> {
    check(&parse(tokenize(src).unwrap()).unwrap())
}

pub(super) fn main_types(src: &str) -> Vec<(String, Type)> {
    let p = check_src(src).unwrap_or_else(|e| panic!("{:?}", e));
    p.functions[0].locals.iter().map(|s| (s.name.clone(), s.ty)).collect()
}

pub(super) fn first_error(src: &str) -> Error {
    check_src(src).unwrap_err().remove(0)
}

fn t(name: &str, ty: Type) -> (String, Type) {
    (name.to_string(), ty)
}

use BaseType::*;

#[test]
fn infers_types_from_assignments() {
    let src = "variables: a, b, c, d\na <- 1\nb <- 2.5\nc <- \"x\"\nd <- a < 2\n";
    assert_eq!(
        main_types(src),
        vec![t("a", Type::Base(Int)), t("b", Type::Base(Real)), t("c", Type::Base(Str)), t("d", Type::Base(Bool))]
    );
}

#[test]
fn integer_widens_to_real() {
    assert_eq!(main_types("variables: x\nx <- 0\nx <- x + 0.5\n"), vec![t("x", Type::Base(Real))]);
}

#[test]
fn input_type_comes_from_later_use() {
    let src = "variables: n\ninput(n)\nIF(n > 0)\n    output(n)\nEND IF\n";
    assert_eq!(main_types(src), vec![t("n", Type::Base(Int))]);
}

#[test]
fn input_only_defaults_to_string() {
    assert_eq!(main_types("variables: s\ninput(s)\noutput(s)\n"), vec![t("s", Type::Base(Str))]);
}

#[test]
fn types_flow_backwards_through_assignments() {
    let src = "variables: a, b\ninput(a)\nb <- a\nb <- b * 2\n";
    assert_eq!(main_types(src), vec![t("a", Type::Base(Int)), t("b", Type::Base(Int))]);
}

#[test]
fn arithmetic_makes_unknown_operands_numbers() {
    assert_eq!(main_types("variables: a, b\ninput(a)\nb <- a * a\n")[0], t("a", Type::Base(Int)));
}

#[test]
fn element_types() {
    let src = "variables: v[3], l[]\nv[1] <- 2\nAPPEND(l, \"x\")\n";
    assert_eq!(main_types(src), vec![t("v", Type::Array(Some(Int))), t("l", Type::List(Some(Str)))]);
}

#[test]
fn string_concatenation() {
    let src = "variables: s, n\nn <- 3\ns <- \"n = \" + n\n";
    assert_eq!(main_types(src)[0], t("s", Type::Base(Str)));
}

#[test]
fn conflict_is_reported() {
    let e = first_error("variables: x\nx <- 1\nx <- \"a\"\n");
    assert_eq!(e.message, "\"x\" is an integer (line 2), but here it gets a string");
    assert_eq!(e.pos.line, 3);
}

#[test]
fn undeclared_variable() {
    let e = first_error("x <- 1\n");
    assert_eq!(e.message, "variable \"x\" is not declared");
    assert_eq!(e.hint.as_deref(), Some("add it to \"variables:\" at the top of the program"));
    assert_eq!((e.pos.line, e.pos.col), (1, 1));
}

#[test]
fn miscased_value_keyword_hint() {
    let e = first_error("variables: b\nb <- true\n");
    assert_eq!(e.hint.as_deref(), Some("keywords are written in UPPERCASE: write TRUE"));
}

#[test]
fn operator_type_errors() {
    assert_eq!(first_error("variables: a\na <- 1 + TRUE\n").message, "cannot use + between an integer and a boolean");
    assert_eq!(first_error("variables: a\na <- 7.5 MOD 2\n").message, "DIV and MOD only work with integers");
    assert_eq!(first_error("variables: a\na <- \"x\" < 1\n").message, "cannot compare a string with an integer");
    assert_eq!(
        first_error("variables: a\na <- 1\nIF(a)\nEND IF\n").message,
        "the condition must be TRUE or FALSE, here it is an integer"
    );
}

#[test]
fn use_before_assignment_in_if_without_else() {
    let e = first_error("variables: x, y\nIF(TRUE)\n    x <- 1\nEND IF\ny <- x\n");
    assert_eq!(e.message, "variable \"x\" might not have a value yet");
    assert_eq!(e.pos.line, 5);
}

#[test]
fn use_before_assignment_in_while() {
    let e = first_error("variables: x, y\nWHILE(FALSE)\n    x <- 1\nEND LOOP\ny <- x\n");
    assert_eq!(e.message, "variable \"x\" might not have a value yet");
}

#[test]
fn assignment_in_both_branches_or_repeat_is_fine() {
    assert!(check_src("variables: x, y\nIF(TRUE)\n    x <- 1\nELSE\n    x <- 2\nEND IF\ny <- x\n").is_ok());
    assert!(check_src("variables: x, y\nREPEAT\n    x <- 1\nWHILE(FALSE)\ny <- x\n").is_ok());
}

#[test]
fn for_variable_must_be_integer() {
    let e = first_error("variables: i\ni <- 0.5\nFOR(i FROM 1 TO 3)\nEND LOOP\n");
    assert_eq!(e.message, "the FOR variable must be an integer, \"i\" is a real");
}

#[test]
fn length_rules() {
    assert_eq!(main_types("variables: v[3], n\nn <- LENGTH(v)\n")[1], t("n", Type::Base(Int)));
    assert_eq!(first_error("variables: m[2, 2], n\nn <- LENGTH(m)\n").message, "LENGTH cannot be used on a matrix");
}

#[test]
fn index_errors() {
    assert_eq!(
        first_error("variables: m[2, 2]\nm[1] <- 0\n").message,
        "\"m\" is a matrix: it needs 2 indices, like m[i, j]"
    );
    assert_eq!(first_error("variables: v[2]\nv <- 1\n").message, "a whole array or list cannot be assigned at once");
    assert_eq!(first_error("variables: x\nx <- 1\nx[1] <- 2\n").message, "\"x\" is not an array: [ ] cannot be used on it");
}

#[test]
fn errors_are_collected_and_sorted() {
    let errors = check_src("a <- 1\nb <- 2\n").unwrap_err();
    assert_eq!(errors.len(), 2);
    assert_eq!((errors[0].pos.line, errors[1].pos.line), (1, 2));
}

#[test]
fn misc_statement_errors() {
    assert_eq!(first_error("RETURN\n").message, "RETURN can only be used inside a FUNCTION or PROCEDURE");
    assert_eq!(first_error("variables: v[2]\noutput(v)\n").message, "output prints only simple values");
    assert_eq!(first_error("variables: v[2]\nAPPEND(v, 1)\n").message, "APPEND works only with lists (declared with [])");
    assert_eq!(first_error("foo(1)\n").message, "function \"foo\" does not exist");
    assert_eq!(first_error("variables: v[]\nlength(v)\n").hint.as_deref(), Some("did you mean LENGTH?"));
}

#[test]
fn declaration_errors() {
    assert_eq!(first_error("variables: a, a\n").message, "\"a\" is declared twice");
    assert_eq!(first_error("variables: LENGTH\n").message, "\"LENGTH\" is a built-in name: choose another one");
    assert_eq!(first_error("GLOBAL variables: g\nvariables: g\n").message, "\"g\" is already declared as GLOBAL");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib checker`
Expected: compile error, `file not found for module checker`.

- [ ] **Step 3: Implement the core (`src/checker/mod.rs`)**

```rust
//! Semantic analysis: name resolution, type inference, specialization of
//! functions per argument types, definite assignment and return checks.
//! Produces the typed program consumed by the compiler.

mod calls;
mod exprs;
mod stmts;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::ast::{Decl, Program, Shape, Stmt, StmtKind};
use crate::errors::{Error, Pos};
use crate::tir::*;

pub const BUILTINS: [&str; 5] = ["input", "output", "LENGTH", "APPEND", "REMOVE"];

/// A variable as the checker sees it. For arrays, matrices and lists `ty`
/// is the element type.
#[derive(Debug, Clone)]
pub(crate) struct Var {
    pub(crate) name: String,
    pub(crate) shape: Shape,
    pub(crate) ty: Option<BaseType>,
    /// where the type was first fixed, for conflict messages
    pub(crate) origin: Option<Pos>,
    pub(crate) conflict: Option<Error>,
    pub(crate) is_param: bool,
}

impl Var {
    pub(crate) fn declared(d: &Decl) -> Var {
        Var { name: d.name.clone(), shape: d.shape.clone(), ty: None, origin: None, conflict: None, is_param: false }
    }

    pub(crate) fn full_type(&self) -> Type {
        match self.shape {
            Shape::Scalar => self.ty.map_or(Type::Unknown, Type::Base),
            Shape::Array(_) => Type::Array(self.ty),
            Shape::Matrix(..) => Type::Matrix(self.ty),
            Shape::List => Type::List(self.ty),
        }
    }
}

/// One specialization: the main program (`sub == None`) or a subprogram
/// checked for one combination of argument types.
pub(crate) struct Instance {
    pub(crate) sub: Option<usize>,
    pub(crate) locals: Vec<Var>,
    pub(crate) ret: Option<BaseType>,
    pub(crate) ret_origin: Option<Pos>,
    pub(crate) final_index: Option<usize>,
}

pub(crate) struct Checker<'p> {
    pub(crate) prog: &'p Program,
    pub(crate) globals: Vec<Var>,
    pub(crate) instances: Vec<Instance>,
    pub(crate) by_key: HashMap<(usize, Vec<Type>), usize>,
    pub(crate) sub_by_name: HashMap<String, usize>,
    /// set whenever a type, a return type or the set of instances grows
    pub(crate) changed: bool,
    /// true only in the last round: errors are reported and IR is kept
    pub(crate) is_final: bool,
    /// true while checking a never-called subprogram: only structural errors count
    pub(crate) names_only: bool,
    pub(crate) errors: Vec<Error>,
    pub(crate) current: usize,
    pub(crate) queue: Vec<usize>,
    pub(crate) visited: HashSet<usize>,
    /// definite assignment state of the current instance's locals
    pub(crate) assigned: Vec<bool>,
    pub(crate) final_queue: Vec<usize>,
}

pub fn check(prog: &Program) -> Result<TProgram, Vec<Error>> {
    let mut c = Checker::new(prog);
    c.check_declarations();
    if !c.errors.is_empty() {
        return Err(c.errors);
    }
    let mut rounds = 0;
    loop {
        loop {
            c.changed = false;
            c.round();
            rounds += 1;
            assert!(rounds < 10_000, "internal error: type inference does not converge");
            if !c.changed {
                break;
            }
        }
        if !c.default_unknowns() {
            break;
        }
    }
    c.is_final = true;
    let program = c.final_round();
    if c.errors.is_empty() {
        Ok(program)
    } else {
        Err(c.errors)
    }
}

impl<'p> Checker<'p> {
    fn new(prog: &'p Program) -> Checker<'p> {
        let main = Instance {
            sub: None,
            locals: prog.locals.iter().map(Var::declared).collect(),
            ret: None,
            ret_origin: None,
            final_index: None,
        };
        Checker {
            prog,
            globals: prog.globals.iter().map(Var::declared).collect(),
            instances: vec![main],
            by_key: HashMap::new(),
            sub_by_name: prog.subprograms.iter().enumerate().map(|(i, s)| (s.name.clone(), i)).collect(),
            changed: false,
            is_final: false,
            names_only: false,
            errors: Vec::new(),
            current: 0,
            queue: Vec::new(),
            visited: HashSet::new(),
            assigned: Vec::new(),
            final_queue: Vec::new(),
        }
    }

    /// Records an error that depends on types. Skipped in never-called subprograms.
    pub(crate) fn report(&mut self, e: Error) {
        if !self.names_only {
            self.report_structural(e);
        }
    }

    /// Records an error that does not depend on types. Only the final round
    /// reports; duplicates are dropped and at most 20 errors are kept.
    pub(crate) fn report_structural(&mut self, e: Error) {
        if self.is_final && self.errors.len() < 20 && !self.errors.contains(&e) {
            self.errors.push(e);
        }
    }

    fn check_declarations(&mut self) {
        let prog = self.prog;
        let mut functions = HashSet::new();
        for s in &prog.subprograms {
            if BUILTINS.contains(&s.name.as_str()) {
                self.errors.push(Error::new(s.pos, format!("\"{}\" is a built-in name: choose another one", s.name)));
            } else if !functions.insert(s.name.clone()) {
                self.errors.push(Error::new(s.pos, format!("function \"{}\" is defined twice", s.name)));
            }
        }
        let globals: Vec<(&str, Pos)> = prog.globals.iter().map(|d| (d.name.as_str(), d.pos)).collect();
        self.check_names(&globals, &[], &functions);
        let main: Vec<(&str, Pos)> = prog.locals.iter().map(|d| (d.name.as_str(), d.pos)).collect();
        self.check_names(&main, &globals, &functions);
        for s in &prog.subprograms {
            let mut names: Vec<(&str, Pos)> = s.params.iter().map(|(n, p)| (n.as_str(), *p)).collect();
            names.extend(s.locals.iter().map(|d| (d.name.as_str(), d.pos)));
            self.check_names(&names, &[], &functions);
        }
    }

    fn check_names(&mut self, names: &[(&str, Pos)], globals: &[(&str, Pos)], functions: &HashSet<String>) {
        let mut seen = HashSet::new();
        for &(name, pos) in names {
            let message = if !seen.insert(name) {
                format!("\"{}\" is declared twice", name)
            } else if BUILTINS.contains(&name) {
                format!("\"{}\" is a built-in name: choose another one", name)
            } else if functions.contains(name) {
                format!("\"{}\" is already the name of a function", name)
            } else if globals.iter().any(|(g, _)| *g == name) {
                format!("\"{}\" is already declared as GLOBAL", name)
            } else {
                continue;
            };
            self.errors.push(Error::new(pos, message));
        }
    }

    /// One inference round: visits the main program and every instance it reaches.
    fn round(&mut self) {
        self.visited.clear();
        self.visited.insert(0);
        self.queue = vec![0];
        while let Some(i) = self.queue.pop() {
            self.visit(i);
        }
    }

    /// Notes that instance `i` is called. In inference rounds it gets queued
    /// for a visit; in the final round it gets its index in the output program.
    pub(crate) fn reach(&mut self, i: usize) -> usize {
        if self.names_only {
            return 0;
        }
        if self.is_final {
            if let Some(k) = self.instances[i].final_index {
                return k;
            }
            let k = self.final_queue.len();
            self.final_queue.push(i);
            self.instances[i].final_index = Some(k);
            return k;
        }
        if self.visited.insert(i) {
            self.queue.push(i);
        }
        0
    }

    pub(crate) fn visit(&mut self, i: usize) -> Vec<TStmt> {
        let prog = self.prog;
        self.current = i;
        self.assigned = self.instances[i].locals.iter().map(|v| v.is_param || v.shape != Shape::Scalar).collect();
        let body = match self.instances[i].sub {
            None => &prog.body,
            Some(s) => &prog.subprograms[s].body,
        };
        let out = self.block(body);
        if let Some(s) = self.instances[i].sub {
            let sub = &prog.subprograms[s];
            if sub.is_function && !always_returns(&sub.body) {
                self.report_structural(
                    Error::new(sub.pos, format!("function \"{}\" can reach END FUNCTION without RETURN", sub.name))
                        .with_hint("add RETURN value at the end"),
                );
            }
        }
        out
    }

    /// Gives the string type to everything whose type is still unknown (for
    /// example a variable only ever read with input). Returns true if anything changed.
    fn default_unknowns(&mut self) -> bool {
        let mut changed = false;
        let visited: Vec<usize> = self.visited.iter().copied().collect();
        for i in visited {
            for v in &mut self.instances[i].locals {
                if v.ty.is_none() {
                    v.ty = Some(BaseType::Str);
                    changed = true;
                }
            }
        }
        for v in &mut self.globals {
            if v.ty.is_none() {
                v.ty = Some(BaseType::Str);
                changed = true;
            }
        }
        changed
    }

    fn final_round(&mut self) -> TProgram {
        self.reach(0);
        let mut functions = Vec::new();
        let mut k = 0;
        while k < self.final_queue.len() {
            let i = self.final_queue[k];
            let body = self.visit(i);
            functions.push(self.function(i, body));
            k += 1;
        }
        self.check_uncalled();
        let mut conflicts: Vec<Error> = Vec::new();
        for &i in &self.final_queue {
            conflicts.extend(self.instances[i].locals.iter().filter_map(|v| v.conflict.clone()));
        }
        conflicts.extend(self.globals.iter().filter_map(|v| v.conflict.clone()));
        for e in conflicts {
            self.report(e);
        }
        self.errors.sort_by_key(|e| (e.pos.line, e.pos.col));
        TProgram { globals: self.globals.iter().map(slot).collect(), functions }
    }

    /// Never-called subprograms are filled in by Task 7.
    fn check_uncalled(&mut self) {}

    fn function(&self, i: usize, body: Vec<TStmt>) -> TFunction {
        let inst = &self.instances[i];
        let (name, n_params, is_function) = match inst.sub {
            None => ("main".to_string(), 0, false),
            Some(s) => {
                let sub = &self.prog.subprograms[s];
                (sub.name.clone(), sub.params.len(), sub.is_function)
            }
        };
        TFunction {
            name,
            n_params,
            locals: inst.locals.iter().map(slot).collect(),
            ret: if is_function { inst.ret } else { None },
            body,
        }
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<Place> {
        if let Some(i) = self.instances[self.current].locals.iter().position(|v| v.name == name) {
            return Some(Place::Local(i as u16));
        }
        self.globals.iter().position(|v| v.name == name).map(|i| Place::Global(i as u16))
    }

    pub(crate) fn var(&self, p: Place) -> &Var {
        match p {
            Place::Local(i) => &self.instances[self.current].locals[i as usize],
            Place::Global(i) => &self.globals[i as usize],
        }
    }

    fn var_mut(&mut self, p: Place) -> &mut Var {
        match p {
            Place::Local(i) => &mut self.instances[self.current].locals[i as usize],
            Place::Global(i) => &mut self.globals[i as usize],
        }
    }

    /// Joins `t` into the type of a variable (or of its elements), widening
    /// integer to real. An incompatible type is recorded as a conflict.
    pub(crate) fn widen(&mut self, p: Place, t: BaseType, pos: Pos) {
        let v = self.var_mut(p);
        let changed = match v.ty {
            None => {
                v.ty = Some(t);
                v.origin = Some(pos);
                true
            }
            Some(old) => match join(old, t) {
                Some(new) => {
                    v.ty = Some(new);
                    new != old
                }
                None => {
                    if v.conflict.is_none() {
                        v.conflict = Some(conflict_error(v, old, t, pos));
                    }
                    false
                }
            },
        };
        if changed {
            self.changed = true;
        }
    }

    /// If `e` is a variable (or an element) whose type is still unknown, gives it type `t`.
    pub(crate) fn hint(&mut self, e: &TExpr, t: BaseType) {
        if e.ty != Type::Unknown {
            return;
        }
        match &e.kind {
            TExprKind::Load(p) | TExprKind::Elem(p, _) => self.widen(*p, t, e.pos),
            _ => {}
        }
    }
}

fn conflict_error(v: &Var, old: BaseType, new: BaseType, pos: Pos) -> Error {
    let line = v.origin.map_or(String::new(), |p| format!(" (line {})", p.line));
    let message = if v.shape == Shape::Scalar {
        format!("\"{}\" is {}{}, but here it gets {}", v.name, old.name(), line, new.name())
    } else {
        format!("the elements of \"{}\" are {}{}, but here they get {}", v.name, old.plural(), line, new.name())
    };
    Error::new(pos, message)
}

fn slot(v: &Var) -> Slot {
    Slot { name: v.name.clone(), ty: v.full_type(), shape: v.shape.clone(), is_param: v.is_param }
}

/// True if every path through `body` ends in RETURN.
pub(crate) fn always_returns(body: &[Stmt]) -> bool {
    body.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::If { branches, else_body: Some(e) } => {
            branches.iter().all(|(_, b)| always_returns(b)) && always_returns(e)
        }
        StmtKind::Repeat(b, _) => always_returns(b),
        _ => false,
    })
}
```

- [ ] **Step 4: Implement expressions (`src/checker/exprs.rs`)**

```rust
//! Checking expressions: their types, conversions and the hints they give
//! to variables of still-unknown type.

use crate::ast::{BinOp, Expr, ExprKind, UnOp};
use crate::errors::{Error, Pos};
use crate::lexer::miscased_value_keyword;
use crate::tir::*;

use super::Checker;

/// The placeholder for an expression whose type is not known (yet, or because of an error).
pub(crate) fn unknown(pos: Pos) -> TExpr {
    TExpr { kind: TExprKind::Int(0), ty: Type::Unknown, pos }
}

fn base(kind: TExprKind, t: BaseType, pos: Pos) -> TExpr {
    TExpr { kind, ty: Type::Base(t), pos }
}

/// Converts an integer expression to real when `target` is real.
pub(crate) fn convert(e: TExpr, target: Option<BaseType>) -> TExpr {
    if target != Some(BaseType::Real) || e.ty != Type::Base(BaseType::Int) {
        return e;
    }
    let pos = e.pos;
    if let TExprKind::Int(n) = &e.kind {
        return base(TExprKind::Real(*n as f64), BaseType::Real, pos);
    }
    base(TExprKind::ToReal(Box::new(e)), BaseType::Real, pos)
}

fn to_str(e: TExpr) -> TExpr {
    if e.ty == Type::Base(BaseType::Str) {
        return e;
    }
    let pos = e.pos;
    base(TExprKind::ToStr(Box::new(e)), BaseType::Str, pos)
}

fn numeric(a: BaseType, b: BaseType) -> Option<BaseType> {
    match (a, b) {
        (BaseType::Int, BaseType::Int) => Some(BaseType::Int),
        (BaseType::Int | BaseType::Real, BaseType::Int | BaseType::Real) => Some(BaseType::Real),
        _ => None,
    }
}

fn symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::IntDiv => "DIV",
        BinOp::Mod => "MOD",
        BinOp::Eq => "=",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::LtEq => "<=",
        BinOp::Gt => ">",
        BinOp::GtEq => ">=",
        BinOp::And => "AND",
        BinOp::Or => "OR",
    }
}

impl<'p> Checker<'p> {
    pub(crate) fn expr(&mut self, e: &Expr) -> TExpr {
        let pos = e.pos;
        match &e.kind {
            ExprKind::Int(n) => base(TExprKind::Int(*n), BaseType::Int, pos),
            ExprKind::Real(r) => base(TExprKind::Real(*r), BaseType::Real, pos),
            ExprKind::Str(s) => base(TExprKind::Str(s.as_str().into()), BaseType::Str, pos),
            ExprKind::Bool(b) => base(TExprKind::Bool(*b), BaseType::Bool, pos),
            ExprKind::Var(name) => match self.lookup(name) {
                None => {
                    let err = self.undeclared(name, pos);
                    self.report_structural(err);
                    unknown(pos)
                }
                Some(p) => {
                    self.check_assigned(p, name, pos);
                    TExpr { kind: TExprKind::Load(p), ty: self.var(p).full_type(), pos }
                }
            },
            ExprKind::Index(name, indices) => self.index(name, indices, pos),
            ExprKind::Binary(op, a, b) => {
                let a = self.expr(a);
                let b = self.expr(b);
                self.binary(*op, a, b, pos)
            }
            ExprKind::Unary(op, x) => {
                let x = self.expr(x);
                self.unary(*op, x, pos)
            }
            ExprKind::Call(name, args) => self.call_expr(name, args, pos),
        }
    }

    pub(crate) fn undeclared(&self, name: &str, pos: Pos) -> Error {
        let e = Error::new(pos, format!("variable \"{}\" is not declared", name));
        if let Some(k) = miscased_value_keyword(name) {
            return e.with_hint(format!("keywords are written in UPPERCASE: write {}", k));
        }
        if self.sub_by_name.contains_key(name) {
            return e.with_hint(format!("\"{}\" is a function: call it with {}(...)", name, name));
        }
        let in_sub = self.instances[self.current].sub.is_some();
        if in_sub && self.prog.locals.iter().any(|d| d.name == name) {
            return e.with_hint(format!(
                "\"{}\" belongs to the main program: to use it here declare it with GLOBAL variables:",
                name
            ));
        }
        e.with_hint(if in_sub {
            "add it to \"variables:\" at the top of the function"
        } else {
            "add it to \"variables:\" at the top of the program"
        })
    }

    /// Checks that an expression is an integer (hinting it if unknown).
    pub(crate) fn int_expr(&mut self, t: TExpr, what: &str) -> TExpr {
        self.hint(&t, BaseType::Int);
        if t.ty != Type::Base(BaseType::Int) && t.ty != Type::Unknown {
            self.report(Error::new(t.pos, format!("{} must be an integer, here it is {}", what, t.ty.name())));
        }
        t
    }

    pub(crate) fn condition(&mut self, e: &Expr) -> TExpr {
        let t = self.expr(e);
        self.hint(&t, BaseType::Bool);
        if t.ty != Type::Base(BaseType::Bool) && t.ty != Type::Unknown {
            self.report(
                Error::new(t.pos, format!("the condition must be TRUE or FALSE, here it is {}", t.ty.name()))
                    .with_hint("to compare values use =, !=, <, >, <= or >="),
            );
        }
        t
    }

    fn index(&mut self, name: &str, indices: &[Expr], pos: Pos) -> TExpr {
        let Some(p) = self.lookup(name) else {
            let err = self.undeclared(name, pos);
            self.report_structural(err);
            return unknown(pos);
        };
        let idx: Vec<TExpr> = indices
            .iter()
            .map(|i| {
                let t = self.expr(i);
                self.int_expr(t, "the index")
            })
            .collect();
        let v = self.var(p);
        let (shape, elem) = (v.shape.clone(), v.ty);
        let elem_ty = elem.map_or(Type::Unknown, Type::Base);
        match shape {
            crate::ast::Shape::Array(_) | crate::ast::Shape::List if idx.len() == 1 => {
                TExpr { kind: TExprKind::Elem(p, idx), ty: elem_ty, pos }
            }
            crate::ast::Shape::Matrix(..) if idx.len() == 2 => TExpr { kind: TExprKind::Elem(p, idx), ty: elem_ty, pos },
            crate::ast::Shape::Matrix(..) => {
                self.report(Error::new(pos, format!("\"{}\" is a matrix: it needs 2 indices, like {}[i, j]", name, name)));
                unknown(pos)
            }
            crate::ast::Shape::Array(_) | crate::ast::Shape::List => {
                self.report(Error::new(pos, format!("\"{}\" needs exactly 1 index, like {}[i]", name, name)));
                unknown(pos)
            }
            crate::ast::Shape::Scalar => {
                if elem.is_none() {
                    // only strings can be indexed, so an unknown variable must be one
                    self.widen(p, BaseType::Str, pos);
                } else if elem != Some(BaseType::Str) {
                    self.report(Error::new(pos, format!("\"{}\" is not an array: [ ] cannot be used on it", name)));
                    return unknown(pos);
                }
                if idx.len() != 1 {
                    self.report(Error::new(pos, format!("a string needs exactly 1 index, like {}[i]", name)));
                    return unknown(pos);
                }
                self.check_assigned(p, name, pos);
                let s = TExpr { kind: TExprKind::Load(p), ty: Type::Base(BaseType::Str), pos };
                let i = idx.into_iter().next().expect("one index");
                base(TExprKind::Char(Box::new(s), Box::new(i)), BaseType::Str, pos)
            }
        }
    }

    fn binary(&mut self, op: BinOp, a: TExpr, b: TExpr, pos: Pos) -> TExpr {
        use BaseType::*;
        use BinOp::*;
        match op {
            And | Or => {
                self.hint(&a, Bool);
                self.hint(&b, Bool);
            }
            IntDiv | Mod => {
                self.hint(&a, Int);
                self.hint(&b, Int);
            }
            Sub | Mul | Div => {
                // numeric-only operators: an unknown operand is at least an integer
                let t = b.ty.base().or(a.ty.base()).unwrap_or(Int);
                self.hint(&a, t);
                self.hint(&b, t);
            }
            _ => {
                if let Some(t) = b.ty.base() {
                    self.hint(&a, t);
                }
                if let Some(t) = a.ty.base() {
                    self.hint(&b, t);
                }
            }
        }
        let (ta, tb) = match (a.ty, b.ty) {
            (Type::Base(x), Type::Base(y)) => (x, y),
            (Type::Unknown, _) | (_, Type::Unknown) => return unknown(pos),
            (x, y) => {
                let collection = if x.base().is_none() { x } else { y };
                self.report(
                    Error::new(pos, format!("cannot use {} on {}", symbol(op), collection.name()))
                        .with_hint("use one element, like v[i]"),
                );
                return unknown(pos);
            }
        };
        let typed = |k: BinOp, operands: BaseType, a: TExpr, b: TExpr, result: BaseType| {
            base(TExprKind::Binary(k, operands, Box::new(a), Box::new(b)), result, pos)
        };
        let mismatch = format!("cannot use {} between {} and {}", symbol(op), ta.name(), tb.name());
        match op {
            Add if ta == Str || tb == Str => typed(Add, Str, to_str(a), to_str(b), Str),
            Add | Sub | Mul => match numeric(ta, tb) {
                Some(t) => typed(op, t, convert(a, Some(t)), convert(b, Some(t)), t),
                None => {
                    self.report(Error::new(pos, mismatch));
                    unknown(pos)
                }
            },
            Div => match numeric(ta, tb) {
                Some(_) => typed(Div, Real, convert(a, Some(Real)), convert(b, Some(Real)), Real),
                None => {
                    self.report(Error::new(pos, mismatch));
                    unknown(pos)
                }
            },
            IntDiv | Mod => {
                if ta == Int && tb == Int {
                    typed(op, Int, a, b, Int)
                } else {
                    self.report(Error::new(pos, "DIV and MOD only work with integers"));
                    unknown(pos)
                }
            }
            Eq | NotEq | Lt | LtEq | Gt | GtEq => {
                let ordered = !matches!(op, Eq | NotEq);
                match numeric(ta, tb) {
                    Some(t) => typed(op, t, convert(a, Some(t)), convert(b, Some(t)), Bool),
                    None if ta == tb && (!ordered || ta == Str) => typed(op, ta, a, b, Bool),
                    None => {
                        self.report(Error::new(pos, format!("cannot compare {} with {}", ta.name(), tb.name())));
                        unknown(pos)
                    }
                }
            }
            And | Or => {
                if ta == Bool && tb == Bool {
                    typed(op, Bool, a, b, Bool)
                } else {
                    self.report(Error::new(pos, format!("{} needs conditions (TRUE/FALSE) on both sides", symbol(op))));
                    unknown(pos)
                }
            }
        }
    }

    fn unary(&mut self, op: UnOp, x: TExpr, pos: Pos) -> TExpr {
        match op {
            UnOp::Neg => {
                self.hint(&x, BaseType::Int);
                // fold -5 and -2.5 into literals (only real literals: the
                // `unknown` placeholder is also an Int(0), but of unknown type)
                if matches!(x.ty, Type::Base(_)) {
                    match &x.kind {
                        TExprKind::Int(n) => return base(TExprKind::Int(-n), BaseType::Int, pos),
                        TExprKind::Real(r) => return base(TExprKind::Real(-r), BaseType::Real, pos),
                        _ => {}
                    }
                }
                match x.ty {
                    Type::Base(t @ (BaseType::Int | BaseType::Real)) => base(TExprKind::Neg(t, Box::new(x)), t, pos),
                    Type::Unknown => unknown(pos),
                    other => {
                        self.report(Error::new(pos, format!("cannot use - on {}", other.name())));
                        unknown(pos)
                    }
                }
            }
            UnOp::Not => {
                self.hint(&x, BaseType::Bool);
                match x.ty {
                    Type::Base(BaseType::Bool) => base(TExprKind::Not(Box::new(x)), BaseType::Bool, pos),
                    Type::Unknown => unknown(pos),
                    other => {
                        self.report(Error::new(pos, format!("NOT needs a condition (TRUE/FALSE), here it is {}", other.name())));
                        unknown(pos)
                    }
                }
            }
        }
    }

    fn call_expr(&mut self, name: &str, args: &[Expr], pos: Pos) -> TExpr {
        match name {
            "LENGTH" => {
                if args.len() != 1 {
                    self.report_structural(Error::new(pos, format!("LENGTH takes 1 value, here it gets {}", args.len())));
                    return unknown(pos);
                }
                let x = self.expr(&args[0]);
                match x.ty {
                    Type::Unknown => {
                        self.hint(&x, BaseType::Str);
                        unknown(pos)
                    }
                    Type::Array(_) | Type::List(_) | Type::Base(BaseType::Str) => {
                        base(TExprKind::Length(Box::new(x)), BaseType::Int, pos)
                    }
                    Type::Matrix(_) => {
                        self.report(
                            Error::new(pos, "LENGTH cannot be used on a matrix")
                                .with_hint("use the sizes written in \"variables:\""),
                        );
                        unknown(pos)
                    }
                    other => {
                        self.report(Error::new(
                            pos,
                            format!("LENGTH works with arrays, lists and strings, not with {}", other.name()),
                        ));
                        unknown(pos)
                    }
                }
            }
            "input" | "output" | "APPEND" | "REMOVE" => {
                self.report_structural(Error::new(
                    pos,
                    format!("{} does not return a value: use it as a statement on its own line", name),
                ));
                unknown(pos)
            }
            _ => self.call(name, args, pos, true),
        }
    }
}
```

- [ ] **Step 5: Implement statements (`src/checker/stmts.rs`)**

```rust
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
                        .with_hint("give it a value (with <- or input) before using it"),
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
                .with_hint(format!("assign one element at a time, like {}[i] <- ...", t.name)),
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
                    .with_hint("assign one element at a time, like v[i] <- ..."),
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
            self.int_expr(e, "the start of FOR")
        };
        let to = {
            let e = self.expr(to);
            self.int_expr(e, "the end of FOR")
        };
        let step = step.map(|s| {
            let e = self.expr(s);
            self.int_expr(e, "the STEP")
        });
        let Some(p) = self.lookup(var) else {
            let err = self.undeclared(var, var_pos);
            self.report_structural(err);
            return None;
        };
        if self.var(p).shape != Shape::Scalar {
            self.report(Error::new(var_pos, "the FOR variable must be a simple variable"));
            return None;
        }
        if self.var(p).ty.is_none() {
            self.widen(p, BaseType::Int, var_pos);
        }
        if let Some(t) = self.var(p).ty {
            if t != BaseType::Int {
                self.report(Error::new(
                    var_pos,
                    format!("the FOR variable must be an integer, \"{}\" is {}", var, t.name()),
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
                                .with_hint("to print an array use a FOR loop"),
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
            "APPEND" | "REMOVE" => {
                if args.len() != 2 {
                    let what = if name == "APPEND" { "the list and the value" } else { "the list and the position" };
                    self.report_structural(Error::new(pos, format!("{} takes 2 values: {}", name, what)));
                    return None;
                }
                let list = self.expr(&args[0]);
                let second = self.expr(&args[1]);
                let Type::List(elem) = list.ty else {
                    if list.ty != Type::Unknown {
                        self.report(Error::new(list.pos, format!("{} works only with lists (declared with [])", name)));
                    }
                    return None;
                };
                if name == "REMOVE" {
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
            "LENGTH" => {
                self.report_structural(Error::new(
                    pos,
                    "LENGTH returns a value: use it in an expression, like n <- LENGTH(v)",
                ));
                None
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
                self.report_structural(Error::new(pos, "RETURN can only be used inside a FUNCTION or PROCEDURE"));
                None
            }
            (Some(false), Some(_)) => {
                self.report_structural(
                    Error::new(pos, "a PROCEDURE does not return values").with_hint("use FUNCTION, or write just RETURN"),
                );
                None
            }
            (Some(true), None) => {
                self.report_structural(Error::new(pos, "in a FUNCTION, RETURN needs a value"));
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
                        format!("a FUNCTION can only return a simple value, not {}", other.name()),
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
        let cur = self.current;
        let (old, origin) = (self.instances[cur].ret, self.instances[cur].ret_origin);
        match old {
            None => {
                self.instances[cur].ret = Some(t);
                self.instances[cur].ret_origin = Some(pos);
                self.changed = true;
            }
            Some(old) => match join(old, t) {
                Some(new) => {
                    if new != old {
                        self.instances[cur].ret = Some(new);
                        self.changed = true;
                    }
                }
                None => self.report(Error::new(
                    pos,
                    format!(
                        "this function already returns {} (line {}), here it returns {}",
                        old.name(),
                        origin.map_or(0, |p| p.line),
                        t.name()
                    ),
                )),
            },
        }
    }
}
```

- [ ] **Step 6: Add the temporary call handler (`src/checker/calls.rs`)**

```rust
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
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test --lib checker`
Expected: 21 passed. Warnings about unused helpers are fine. If a borrow-checker error points at a `match` on `e.kind` / `t.kind`, change it to match on `&...kind` and dereference the bindings; the logic stays the same.

- [ ] **Step 8: Commit**

```powershell
git add src/lib.rs src/checker
git commit -m "Add checker: scopes, type inference, expressions and statements" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Checker: functions and procedures

**How calls work:**
- **Specialization**:
  - A call computes the argument types and looks up (or creates) the instance for the key `(subprogram, argument types)`.
  - Unknown argument types are part of the key too. As types get known in later rounds, the key changes and a new instance appears.
  - Only the instances reached in the final round are emitted.
- **Back-propagation**: after the call, whatever the instance learned about its parameters flows back to the arguments.
  - A scalar argument of unknown type gets *hinted*.
  - A collection argument's element type gets *widened*: arrays are shared, so their element types must agree.
- **Never-called subprograms**: they are still visited once in the final round with `names_only = true`. Undeclared names and other structural errors are reported, but type errors are not.

**Files:**
- Modify: `src/checker/calls.rs`, `src/checker/mod.rs`, `src/checker/tests.rs`

**Interfaces:**
- Consumes: everything from Task 6.
- Produces: the final `call(&mut self, name, args, pos, in_expr) -> TExpr`. For `in_expr == false` its `ty` is `Unknown` and only `kind` is used.

- [ ] **Step 1: Write the failing tests**

Append to `src/checker/tests.rs`:
```rust
#[test]
fn function_return_type_is_inferred() {
    let src = "variables: r\nFUNCTION double(x)\n    RETURN x * 2\nEND FUNCTION\nr <- double(4)\n";
    let p = check_src(src).unwrap();
    assert_eq!(p.functions[0].locals[0].ty, Type::Base(Int));
    assert_eq!(p.functions.len(), 2);
    assert_eq!(p.functions[1].name, "double");
    assert_eq!(p.functions[1].ret, Some(Int));
}

#[test]
fn functions_are_specialized_per_argument_types() {
    let src = "variables: a, b\nFUNCTION double(x)\n    RETURN x * 2\nEND FUNCTION\na <- double(4)\nb <- double(1.5)\n";
    let p = check_src(src).unwrap();
    assert_eq!(main_types(src), vec![t("a", Type::Base(Int)), t("b", Type::Base(Real))]);
    assert_eq!(p.functions.len(), 3);
}

#[test]
fn recursion() {
    let src = "variables: r\nFUNCTION fib(n)\n    IF(n < 2)\n        RETURN n\n    END IF\n    RETURN fib(n - 1) + fib(n - 2)\nEND FUNCTION\nr <- fib(10)\n";
    assert_eq!(main_types(src), vec![t("r", Type::Base(Int))]);
}

#[test]
fn parameter_types_flow_back_to_arguments() {
    let src = "variables: n, r\nFUNCTION square(x)\n    RETURN x * x\nEND FUNCTION\ninput(n)\nr <- square(n)\n";
    assert_eq!(main_types(src), vec![t("n", Type::Base(Int)), t("r", Type::Base(Int))]);
}

#[test]
fn array_element_type_flows_back_from_procedure() {
    let src = "variables: v[3]\nPROCEDURE fill(a)\n    variables: i\n    FOR(i FROM 1 TO 3)\n        a[i] <- i * 1.5\n    END LOOP\nEND PROCEDURE\nfill(v)\n";
    let p = check_src(src).unwrap();
    assert_eq!(p.functions[0].locals[0].ty, Type::Array(Some(Real)));
    assert_eq!(p.functions.len(), 2);
}

#[test]
fn call_errors() {
    let double = "FUNCTION double(x)\n    RETURN x * 2\nEND FUNCTION\n";
    assert_eq!(
        first_error(&format!("variables: r\n{}r <- double(1, 2)\n", double)).message,
        "\"double\" takes 1 value, here it gets 2"
    );
    assert_eq!(
        first_error("variables: r\nPROCEDURE p()\nEND PROCEDURE\nr <- p()\n").message,
        "PROCEDURE \"p\" does not return a value"
    );
    assert_eq!(
        first_error("variables: r\nFUNCTION f(x)\n    IF(x > 0)\n        RETURN 1\n    END IF\nEND FUNCTION\nr <- f(2)\n").message,
        "function \"f\" can reach END FUNCTION without RETURN"
    );
    assert_eq!(
        first_error("PROCEDURE p()\n    RETURN 1\nEND PROCEDURE\np()\n").message,
        "a PROCEDURE does not return values"
    );
}

#[test]
fn main_variables_are_not_visible_in_functions() {
    let e = first_error("variables: x\nPROCEDURE p()\n    x <- 1\nEND PROCEDURE\np()\n");
    assert_eq!(e.message, "variable \"x\" is not declared");
    assert_eq!(
        e.hint.as_deref(),
        Some("\"x\" belongs to the main program: to use it here declare it with GLOBAL variables:")
    );
    assert!(check_src("GLOBAL variables: x\nPROCEDURE p()\n    x <- 1\nEND PROCEDURE\np()\n").is_ok());
}

#[test]
fn uncalled_subprograms_are_checked_for_names_only() {
    assert_eq!(
        first_error("PROCEDURE never()\n    y <- 1\nEND PROCEDURE\n").message,
        "variable \"y\" is not declared"
    );
    assert!(check_src("FUNCTION unused(x)\n    RETURN x * \"a\"\nEND FUNCTION\n").is_ok());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib checker`
Expected: the 8 new tests FAIL with `function "double" does not exist` (and similar).

- [ ] **Step 3: Replace `call` in `src/checker/calls.rs`**

Replace the whole file with:
```rust
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
```

- [ ] **Step 4: Check never-called subprograms**

In `src/checker/mod.rs`, replace the empty `check_uncalled` with:
```rust
    /// Visits every subprogram that the final round never reached, so that
    /// names are checked even in code that is not called yet.
    fn check_uncalled(&mut self) {
        let prog = self.prog;
        for s in 0..prog.subprograms.len() {
            let reached = self.instances.iter().any(|i| i.sub == Some(s) && i.final_index.is_some());
            if reached {
                continue;
            }
            let key = vec![Type::Unknown; prog.subprograms[s].params.len()];
            let inst = self.instance(s, key);
            self.names_only = true;
            self.visit(inst);
            self.names_only = false;
        }
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib checker`
Expected: 29 passed.

- [ ] **Step 6: Commit**

```powershell
git add src/checker
git commit -m "Check function calls with per-type specialization" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Bytecode and VM core

**Files:**
- Create: `src/bytecode.rs`, `src/vm.rs`
- Modify: `src/lib.rs` (add `pub mod bytecode;` and `pub mod vm;`)

**Interfaces:**
- Consumes: `errors::{Error, Pos}`, `tir::BaseType`.
- Produces: `bytecode::{Reg (= u16), Cmp, Op, Function, Module}`. Task 9 adds more `Op` variants.
- Produces: `vm::run(&Module, &mut dyn BufRead, &mut dyn Write) -> Result<(), Error>`, `vm::format_real(f64) -> String`.
- **Run-time errors** carry `positions[ip]` of the failing instruction.
- **Registers**: a function's registers are `[base, base + n_regs)` in one shared `Vec<Value>`. Register k of a function holds its local slot k.

- [ ] **Step 1: Write the bytecode definitions**

Create `src/bytecode.rs`:
```rust
//! The instruction set of the VM. Instructions name registers of the
//! current function; jump targets are instruction indices.

use std::rc::Rc;

use crate::errors::Pos;
use crate::tir::BaseType;

pub type Reg = u16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    LoadInt(Reg, i64),
    LoadReal(Reg, f64),
    LoadBool(Reg, bool),
    /// load `Module::strings[index]`
    LoadStr(Reg, u32),
    /// dst, src
    Move(Reg, Reg),
    // dst, a, b
    AddI(Reg, Reg, Reg),
    SubI(Reg, Reg, Reg),
    MulI(Reg, Reg, Reg),
    DivI(Reg, Reg, Reg),
    ModI(Reg, Reg, Reg),
    AddR(Reg, Reg, Reg),
    SubR(Reg, Reg, Reg),
    MulR(Reg, Reg, Reg),
    DivR(Reg, Reg, Reg),
    Concat(Reg, Reg, Reg),
    // dst, src
    NegI(Reg, Reg),
    NegR(Reg, Reg),
    Not(Reg, Reg),
    ToReal(Reg, Reg),
    ToStr(Reg, Reg),
    // comparison, dst, a, b
    CmpI(Cmp, Reg, Reg, Reg),
    CmpR(Cmp, Reg, Reg, Reg),
    CmpS(Cmp, Reg, Reg, Reg),
    CmpB(Cmp, Reg, Reg, Reg),
    Jump(u32),
    JumpIfFalse(Reg, u32),
    JumpIfTrue(Reg, u32),
    /// fails if step is 0; jumps to `exit` if the loop must not run at all
    ForInit { var: Reg, end: Reg, step: Reg, exit: u32 },
    /// var += step; jumps back to `body` while the loop continues
    ForStep { var: Reg, end: Reg, step: Reg, body: u32 },
    /// print without newline
    Print(Reg),
    PrintLn,
    RetNone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub n_params: u16,
    pub n_regs: u16,
    pub code: Vec<Op>,
    /// source position of each instruction, for run-time errors
    pub positions: Vec<Pos>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Module {
    /// index 0 is the main program
    pub functions: Vec<Function>,
    pub strings: Vec<Rc<str>>,
    /// global variable names, for run-time error messages
    pub globals: Vec<String>,
}
```

- [ ] **Step 2: Write the failing VM tests**

Add `pub mod bytecode;` and `pub mod vm;` to `src/lib.rs`. Create `src/vm.rs` containing only:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{Cmp, Function, Module, Op::*};
    use crate::errors::Pos;

    /// Runs one main function. Instruction k reports errors at line k + 1.
    pub(crate) fn run_code(code: Vec<Op>, n_regs: u16, strings: &[&str], input: &str) -> Result<String, Error> {
        let positions = (0..code.len()).map(|k| Pos::new(k as u32 + 1, 0, 0)).collect();
        let module = Module {
            functions: vec![Function { name: "main".into(), n_params: 0, n_regs, code, positions }],
            strings: strings.iter().map(|s| Rc::from(*s)).collect(),
            globals: vec![],
        };
        let mut out = Vec::new();
        run(&module, &mut input.as_bytes(), &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn integer_and_real_arithmetic() {
        let code = vec![
            LoadInt(0, 7), LoadInt(1, 2),
            AddI(2, 0, 1), Print(2), PrintLn,
            DivI(2, 0, 1), Print(2), PrintLn,
            ModI(2, 0, 1), Print(2), PrintLn,
            ToReal(3, 0), ToReal(4, 1), DivR(5, 3, 4), Print(5), PrintLn,
            LoadReal(5, 3.0), Print(5), PrintLn,
            NegI(2, 0), Print(2), PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 6, &[], "").unwrap(), "9\n3\n1\n3.5\n3.0\n-7\n");
    }

    #[test]
    fn negative_div_and_mod_truncate() {
        let code = vec![LoadInt(0, -7), LoadInt(1, 2), DivI(2, 0, 1), ModI(3, 0, 1), Print(2), Print(3), PrintLn, RetNone];
        assert_eq!(run_code(code, 4, &[], "").unwrap(), "-3-1\n");
    }

    #[test]
    fn overflow_is_an_error() {
        let code = vec![LoadInt(0, i64::MAX), LoadInt(1, 1), AddI(2, 0, 1), RetNone];
        let e = run_code(code, 3, &[], "").unwrap_err();
        assert_eq!(e.message, "number too large: the result exceeds the integer limits");
        assert_eq!(e.pos.line, 3);
    }

    #[test]
    fn division_by_zero_is_an_error() {
        let code = vec![LoadInt(0, 1), LoadInt(1, 0), DivI(2, 0, 1), RetNone];
        assert_eq!(run_code(code, 3, &[], "").unwrap_err().message, "division by zero");
        let code = vec![LoadReal(0, 1.0), LoadReal(1, 0.0), DivR(2, 0, 1), RetNone];
        assert_eq!(run_code(code, 3, &[], "").unwrap_err().message, "division by zero");
    }

    #[test]
    fn while_style_loop() {
        let code = vec![
            LoadInt(0, 0),            // 0: s
            LoadInt(1, 1),            // 1: i
            LoadInt(2, 5),            // 2: limit
            LoadInt(3, 1),            // 3: one
            CmpI(Cmp::LtEq, 4, 1, 2), // 4
            JumpIfFalse(4, 9),        // 5
            AddI(0, 0, 1),            // 6
            AddI(1, 1, 3),            // 7
            Jump(4),                  // 8
            Print(0),                 // 9
            PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 5, &[], "").unwrap(), "15\n");
    }

    #[test]
    fn for_loop_with_negative_step() {
        let code = vec![
            LoadInt(0, 10), LoadInt(1, 0), LoadInt(2, -2),
            ForInit { var: 0, end: 1, step: 2, exit: 7 },
            Print(0), PrintLn,
            ForStep { var: 0, end: 1, step: 2, body: 4 },
            RetNone,
        ];
        assert_eq!(run_code(code, 3, &[], "").unwrap(), "10\n8\n6\n4\n2\n0\n");
    }

    #[test]
    fn for_loop_that_never_runs_and_zero_step() {
        let skip = vec![
            LoadInt(0, 5), LoadInt(1, 1), LoadInt(2, 1),
            ForInit { var: 0, end: 1, step: 2, exit: 6 },
            Print(0),
            ForStep { var: 0, end: 1, step: 2, body: 4 },
            RetNone,
        ];
        assert_eq!(run_code(skip, 3, &[], "").unwrap(), "");
        let zero = vec![LoadInt(0, 1), LoadInt(1, 5), LoadInt(2, 0), ForInit { var: 0, end: 1, step: 2, exit: 4 }, RetNone];
        assert_eq!(run_code(zero, 3, &[], "").unwrap_err().message, "STEP cannot be 0");
    }

    #[test]
    fn strings_booleans_and_comparisons() {
        let code = vec![
            LoadStr(0, 0), LoadReal(1, 7.5), ToStr(2, 1), Concat(3, 0, 2), Print(3), PrintLn,
            LoadStr(4, 1), CmpS(Cmp::Lt, 5, 0, 4), Print(5), PrintLn,
            Not(5, 5), Print(5), PrintLn,
            LoadBool(6, true), CmpB(Cmp::Eq, 7, 5, 6), Print(7), PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 8, &["Grade: ", "Z"], "").unwrap(), "Grade: 7.5\nTRUE\nFALSE\nFALSE\n");
    }

    #[test]
    fn real_formatting() {
        assert_eq!(format_real(3.0), "3.0");
        assert_eq!(format_real(2.5), "2.5");
        assert_eq!(format_real(-0.25), "-0.25");
        assert_eq!(format_real(0.1 + 0.2), "0.30000000000000004");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib vm`
Expected: compile error, `cannot find function run`.

- [ ] **Step 4: Implement**

Add above the test module in `src/vm.rs`:
```rust
//! The virtual machine: executes the register bytecode produced by the compiler.

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

use crate::bytecode::{Cmp, Module, Op};
use crate::errors::Error;

#[derive(Debug, Clone)]
pub enum Value {
    /// a register or element that has no value yet
    Null,
    Int(i64),
    Real(f64),
    Bool(bool),
    Str(Rc<str>),
    /// arrays and lists
    Seq(Rc<RefCell<Vec<Value>>>),
    Matrix(Rc<RefCell<Matrix>>),
}

#[derive(Debug)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Value>,
}

const OVERFLOW: &str = "number too large: the result exceeds the integer limits";

/// Shortest representation of a real, always with a decimal point.
pub fn format_real(r: f64) -> String {
    let s = r.to_string();
    if r.is_finite() && !s.contains('.') {
        s + ".0"
    } else {
        s
    }
}

fn display(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Real(r) => format_real(*r),
        Value::Bool(true) => "TRUE".to_string(),
        Value::Bool(false) => "FALSE".to_string(),
        Value::Str(s) => s.to_string(),
        Value::Null | Value::Seq(_) | Value::Matrix(_) => panic!("internal error: cannot display {:?}", v),
    }
}

#[inline(always)]
fn int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        other => panic!("internal error: expected an integer, found {:?}", other),
    }
}

#[inline(always)]
fn real(v: &Value) -> f64 {
    match v {
        Value::Real(r) => *r,
        other => panic!("internal error: expected a real, found {:?}", other),
    }
}

#[inline(always)]
fn boolean(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => panic!("internal error: expected a boolean, found {:?}", other),
    }
}

#[inline(always)]
fn string(v: &Value) -> &str {
    match v {
        Value::Str(s) => s,
        other => panic!("internal error: expected a string, found {:?}", other),
    }
}

fn compare<T: PartialOrd + ?Sized>(c: Cmp, a: &T, b: &T) -> bool {
    match c {
        Cmp::Eq => a == b,
        Cmp::NotEq => a != b,
        Cmp::Lt => a < b,
        Cmp::LtEq => a <= b,
        Cmp::Gt => a > b,
        Cmp::GtEq => a >= b,
    }
}

struct Frame {
    func: usize,
    ip: usize,
    base: usize,
    dst: u16,
}

pub fn run(module: &Module, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
    let mut vm = Vm {
        module,
        regs: Vec::with_capacity(1 << 12),
        globals: vec![Value::Null; module.globals.len()],
        frames: Vec::new(),
    };
    let result = vm.execute(input, output);
    let _ = output.flush();
    result
}

struct Vm<'m> {
    module: &'m Module,
    regs: Vec<Value>,
    globals: Vec<Value>,
    frames: Vec<Frame>,
}

impl<'m> Vm<'m> {
    fn error_at(&self, func: usize, ip: usize, message: impl Into<String>) -> Error {
        Error::new(self.module.functions[func].positions[ip - 1], message)
    }

    fn execute(&mut self, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
        let m = self.module;
        let mut func = 0usize;
        let mut code: &[Op] = &m.functions[0].code;
        let mut ip = 0usize;
        let mut base = 0usize;
        self.regs.resize(m.functions[0].n_regs as usize, Value::Null);

        macro_rules! reg {
            ($r:expr) => {
                self.regs[base + $r as usize]
            };
        }
        macro_rules! fail {
            ($msg:expr) => {
                return Err(self.error_at(func, ip, $msg))
            };
        }

        loop {
            let op = code[ip];
            ip += 1;
            match op {
                Op::LoadInt(d, n) => reg!(d) = Value::Int(n),
                Op::LoadReal(d, r) => reg!(d) = Value::Real(r),
                Op::LoadBool(d, b) => reg!(d) = Value::Bool(b),
                Op::LoadStr(d, i) => reg!(d) = Value::Str(m.strings[i as usize].clone()),
                Op::Move(d, s) => {
                    let v = reg!(s).clone();
                    reg!(d) = v;
                }
                Op::AddI(d, a, b) => match int(&reg!(a)).checked_add(int(&reg!(b))) {
                    Some(n) => reg!(d) = Value::Int(n),
                    None => fail!(OVERFLOW),
                },
                Op::SubI(d, a, b) => match int(&reg!(a)).checked_sub(int(&reg!(b))) {
                    Some(n) => reg!(d) = Value::Int(n),
                    None => fail!(OVERFLOW),
                },
                Op::MulI(d, a, b) => match int(&reg!(a)).checked_mul(int(&reg!(b))) {
                    Some(n) => reg!(d) = Value::Int(n),
                    None => fail!(OVERFLOW),
                },
                Op::DivI(d, a, b) => {
                    let (x, y) = (int(&reg!(a)), int(&reg!(b)));
                    if y == 0 {
                        fail!("division by zero");
                    }
                    match x.checked_div(y) {
                        Some(n) => reg!(d) = Value::Int(n),
                        None => fail!(OVERFLOW),
                    }
                }
                Op::ModI(d, a, b) => {
                    let (x, y) = (int(&reg!(a)), int(&reg!(b)));
                    if y == 0 {
                        fail!("division by zero");
                    }
                    match x.checked_rem(y) {
                        Some(n) => reg!(d) = Value::Int(n),
                        None => fail!(OVERFLOW),
                    }
                }
                Op::AddR(d, a, b) => reg!(d) = Value::Real(real(&reg!(a)) + real(&reg!(b))),
                Op::SubR(d, a, b) => reg!(d) = Value::Real(real(&reg!(a)) - real(&reg!(b))),
                Op::MulR(d, a, b) => reg!(d) = Value::Real(real(&reg!(a)) * real(&reg!(b))),
                Op::DivR(d, a, b) => {
                    let y = real(&reg!(b));
                    if y == 0.0 {
                        fail!("division by zero");
                    }
                    reg!(d) = Value::Real(real(&reg!(a)) / y);
                }
                Op::Concat(d, a, b) => {
                    let mut t = String::with_capacity(string(&reg!(a)).len() + string(&reg!(b)).len());
                    t.push_str(string(&reg!(a)));
                    t.push_str(string(&reg!(b)));
                    reg!(d) = Value::Str(t.into());
                }
                Op::NegI(d, s) => match int(&reg!(s)).checked_neg() {
                    Some(n) => reg!(d) = Value::Int(n),
                    None => fail!(OVERFLOW),
                },
                Op::NegR(d, s) => reg!(d) = Value::Real(-real(&reg!(s))),
                Op::Not(d, s) => reg!(d) = Value::Bool(!boolean(&reg!(s))),
                Op::ToReal(d, s) => reg!(d) = Value::Real(int(&reg!(s)) as f64),
                Op::ToStr(d, s) => {
                    let t = display(&reg!(s));
                    reg!(d) = Value::Str(t.into());
                }
                Op::CmpI(c, d, a, b) => {
                    let r = compare(c, &int(&reg!(a)), &int(&reg!(b)));
                    reg!(d) = Value::Bool(r);
                }
                Op::CmpR(c, d, a, b) => {
                    let r = compare(c, &real(&reg!(a)), &real(&reg!(b)));
                    reg!(d) = Value::Bool(r);
                }
                Op::CmpS(c, d, a, b) => {
                    let r = compare::<str>(c, string(&reg!(a)), string(&reg!(b)));
                    reg!(d) = Value::Bool(r);
                }
                Op::CmpB(c, d, a, b) => {
                    let r = compare(c, &boolean(&reg!(a)), &boolean(&reg!(b)));
                    reg!(d) = Value::Bool(r);
                }
                Op::Jump(t) => ip = t as usize,
                Op::JumpIfFalse(c, t) => {
                    if !boolean(&reg!(c)) {
                        ip = t as usize;
                    }
                }
                Op::JumpIfTrue(c, t) => {
                    if boolean(&reg!(c)) {
                        ip = t as usize;
                    }
                }
                Op::ForInit { var, end, step, exit } => {
                    let s = int(&reg!(step));
                    if s == 0 {
                        fail!("STEP cannot be 0");
                    }
                    let (v, e) = (int(&reg!(var)), int(&reg!(end)));
                    if (s > 0 && v > e) || (s < 0 && v < e) {
                        ip = exit as usize;
                    }
                }
                Op::ForStep { var, end, step, body } => {
                    let s = int(&reg!(step));
                    // on overflow the counter is past any end value: the loop is over
                    if let Some(v) = int(&reg!(var)).checked_add(s) {
                        let e = int(&reg!(end));
                        reg!(var) = Value::Int(v);
                        if (s > 0 && v <= e) || (s < 0 && v >= e) {
                            ip = body as usize;
                        }
                    }
                }
                Op::Print(s) => {
                    let t = display(&reg!(s));
                    let _ = output.write_all(t.as_bytes());
                }
                Op::PrintLn => {
                    let _ = output.write_all(b"\n");
                }
                Op::RetNone => {
                    let Some(fr) = self.frames.pop() else { return Ok(()) };
                    self.regs.truncate(base);
                    func = fr.func;
                    code = &m.functions[func].code;
                    ip = fr.ip;
                    base = fr.base;
                    let _ = fr.dst;
                }
            }
        }
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib vm`
Expected: 9 passed.

- [ ] **Step 6: Commit**

```powershell
git add src/lib.rs src/bytecode.rs src/vm.rs
git commit -m "Add bytecode and VM core" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: VM: globals, collections, calls and input

**Files:**
- Modify: `src/bytecode.rs`, `src/vm.rs`

**Interfaces:**
- Produces new `Op` variants: `GetGlobal(Reg, u16)`, `SetGlobal(u16, Reg)`, `NewArray(Reg, u32)`, `NewMatrix(Reg, u32, u32)`, `NewList(Reg)`, `GetElem(dst, coll, i)`, `SetElem(coll, i, src)`, `GetElem2(dst, coll, i, j)`, `SetElem2(coll, i, j, src)`, `CharAt(dst, s, i)`, `Length(dst, x)`, `Append(list, src)`, `Remove(list, i)`, `Call { func: u32, first: Reg, argc: u16, dst: Reg }`, `Ret(Reg)`, `Read(Reg, BaseType)`.
- **`Call` semantics**:
  - Arguments are the `argc` consecutive registers starting at `first` in the caller.
  - They are copied into registers `0..argc` of the callee.
  - `Ret(r)` stores the value into the caller's `dst`. `RetNone` stores nothing.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `src/vm.rs`:
```rust
    use crate::tir::BaseType;

    #[test]
    fn arrays_and_lengths() {
        let code = vec![
            NewArray(0, 3), LoadInt(1, 2), LoadInt(2, 42),
            SetElem(0, 1, 2), GetElem(3, 0, 1), Print(3), PrintLn,
            Length(4, 0), Print(4), PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 5, &[], "").unwrap(), "42\n3\n");
    }

    #[test]
    fn element_errors() {
        let out_of_range = vec![NewArray(0, 3), LoadInt(1, 4), GetElem(2, 0, 1), RetNone];
        let e = run_code(out_of_range, 3, &[], "").unwrap_err();
        assert_eq!(e.message, "index 4 is out of range: valid positions are 1 to 3");
        assert_eq!(e.pos.line, 3);
        let unset = vec![NewArray(0, 3), LoadInt(1, 1), GetElem(2, 0, 1), RetNone];
        assert_eq!(run_code(unset, 3, &[], "").unwrap_err().message, "this element has no value yet");
    }

    #[test]
    fn matrices() {
        let code = vec![
            NewMatrix(0, 2, 3), LoadInt(1, 2), LoadInt(2, 3), LoadInt(3, 9),
            SetElem2(0, 1, 2, 3), GetElem2(4, 0, 1, 2), Print(4), PrintLn,
            LoadInt(1, 3), GetElem2(4, 0, 1, 2),
            RetNone,
        ];
        let e = run_code(code, 5, &[], "").unwrap_err();
        assert_eq!(e.message, "row 3 is out of range: valid rows are 1 to 2");
    }

    #[test]
    fn lists() {
        let code = vec![
            NewList(0), LoadStr(1, 0), Append(0, 1), LoadStr(1, 1), Append(0, 1), LoadStr(1, 2), Append(0, 1),
            LoadInt(2, 2), Remove(0, 2),
            Length(3, 0), Print(3), PrintLn,
            GetElem(4, 0, 2), Print(4), PrintLn,
            LoadInt(2, 9), Remove(0, 2),
            RetNone,
        ];
        let e = run_code(code, 5, &["a", "b", "c"], "").unwrap_err();
        assert_eq!(e.message, "index 9 is out of range: valid positions are 1 to 2");
        let empty = vec![NewList(0), LoadInt(1, 1), GetElem(2, 0, 1), RetNone];
        assert_eq!(run_code(empty, 3, &[], "").unwrap_err().message, "index 1 is out of range: the list is empty");
    }

    #[test]
    fn list_output_before_error() {
        let code = vec![
            NewList(0), LoadStr(1, 0), Append(0, 1), Length(2, 0), Print(2), PrintLn, RetNone,
        ];
        assert_eq!(run_code(code, 3, &["x"], "").unwrap(), "1\n");
    }

    #[test]
    fn characters_of_strings() {
        let code = vec![LoadStr(0, 0), LoadInt(1, 2), CharAt(2, 0, 1), Print(2), Length(3, 0), Print(3), PrintLn, RetNone];
        assert_eq!(run_code(code, 4, &["città"], "").unwrap(), "i5\n");
    }

    fn two_functions(main: Vec<Op>, main_regs: u16, f: Vec<Op>, f_regs: u16) -> Module {
        let pos = |n: usize| (0..n).map(|k| Pos::new(k as u32 + 1, 0, 0)).collect();
        Module {
            functions: vec![
                Function { name: "main".into(), n_params: 0, n_regs: main_regs, positions: pos(main.len()), code: main },
                Function { name: "f".into(), n_params: 1, n_regs: f_regs, positions: pos(f.len()), code: f },
            ],
            strings: vec![],
            globals: vec!["g".into()],
        }
    }

    fn run_module(m: &Module, input: &str) -> Result<String, Error> {
        let mut out = Vec::new();
        run(m, &mut input.as_bytes(), &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn calls_and_returns() {
        // f(x) = x * 2
        let f = vec![LoadInt(1, 2), MulI(1, 0, 1), Ret(1)];
        let main = vec![LoadInt(1, 21), Call { func: 1, first: 1, argc: 1, dst: 0 }, Print(0), PrintLn, RetNone];
        assert_eq!(run_module(&two_functions(main, 2, f, 2), "").unwrap(), "42\n");
    }

    #[test]
    fn runaway_recursion_is_an_error() {
        let f = vec![Call { func: 1, first: 0, argc: 1, dst: 0 }, RetNone];
        let main = vec![LoadInt(0, 1), Call { func: 1, first: 0, argc: 1, dst: 0 }, RetNone];
        let e = run_module(&two_functions(main, 1, f, 1), "").unwrap_err();
        assert_eq!(e.message, "too many nested calls (maybe a recursion that never ends)");
    }

    #[test]
    fn globals() {
        let main = vec![LoadInt(0, 5), SetGlobal(0, 0), GetGlobal(1, 0), Print(1), PrintLn, RetNone];
        assert_eq!(run_module(&two_functions(main, 2, vec![RetNone], 1), "").unwrap(), "5\n");
        let unset = vec![GetGlobal(0, 0), RetNone];
        let e = run_module(&two_functions(unset, 1, vec![RetNone], 1), "").unwrap_err();
        assert_eq!(e.message, "variable \"g\" has no value yet");
    }

    #[test]
    fn read_converts_and_trims() {
        let code = vec![
            Read(0, BaseType::Int), Read(1, BaseType::Real), Read(2, BaseType::Str), Read(3, BaseType::Bool),
            Print(0), Print(1), Print(2), Print(3), PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 4, &[], "  42 \r\n3,5\nhello world\r\ntrue\n").unwrap(), "423.5hello worldTRUE\n");
        let bad = vec![Read(0, BaseType::Int), RetNone];
        assert_eq!(run_code(bad.clone(), 1, &[], "abc\n").unwrap_err().message, "expected an integer, read \"abc\"");
        assert_eq!(
            run_code(bad, 1, &[], "").unwrap_err().message,
            "the input is over: the program expected another value"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib vm`
Expected: compile error, `no variant named NewArray`.

- [ ] **Step 3: Add the instructions**

In `src/bytecode.rs`, add these variants to `enum Op`, just before `RetNone`:
```rust
    GetGlobal(Reg, u16),
    SetGlobal(u16, Reg),
    /// new array of n empty elements
    NewArray(Reg, u32),
    /// new rows × cols matrix
    NewMatrix(Reg, u32, u32),
    NewList(Reg),
    /// dst, collection, index (1-based)
    GetElem(Reg, Reg, Reg),
    /// collection, index, src
    SetElem(Reg, Reg, Reg),
    /// dst, matrix, row, column
    GetElem2(Reg, Reg, Reg, Reg),
    /// matrix, row, column, src
    SetElem2(Reg, Reg, Reg, Reg),
    /// dst, string, index
    CharAt(Reg, Reg, Reg),
    /// dst, array/list/string
    Length(Reg, Reg),
    /// list, src
    Append(Reg, Reg),
    /// list, index
    Remove(Reg, Reg),
    /// copies `argc` registers from `first` into the callee's registers 0..argc
    Call { func: u32, first: Reg, argc: u16, dst: Reg },
    Ret(Reg),
    Read(Reg, BaseType),
```

- [ ] **Step 4: Implement them in the VM**

In `src/vm.rs`, add the import `use crate::tir::BaseType;` and these constants and helpers next to `OVERFLOW`:
```rust
const MAX_CALL_DEPTH: usize = 100_000;

fn seq(v: &Value) -> &Rc<RefCell<Vec<Value>>> {
    match v {
        Value::Seq(s) => s,
        other => panic!("internal error: expected an array or list, found {:?}", other),
    }
}

fn matrix(v: &Value) -> &Rc<RefCell<Matrix>> {
    match v {
        Value::Matrix(m) => m,
        other => panic!("internal error: expected a matrix, found {:?}", other),
    }
}

/// 1-based index → 0-based position, or the error message.
fn position(i: i64, len: usize) -> Result<usize, String> {
    if i >= 1 && (i as u64) <= len as u64 {
        Ok(i as usize - 1)
    } else if len == 0 {
        Err(format!("index {} is out of range: the list is empty", i))
    } else {
        Err(format!("index {} is out of range: valid positions are 1 to {}", i, len))
    }
}

fn cell(i: i64, j: i64, m: &Matrix) -> Result<usize, String> {
    if i < 1 || i as u64 > m.rows as u64 {
        return Err(format!("row {} is out of range: valid rows are 1 to {}", i, m.rows));
    }
    if j < 1 || j as u64 > m.cols as u64 {
        return Err(format!("column {} is out of range: valid columns are 1 to {}", j, m.cols));
    }
    Ok((i as usize - 1) * m.cols + (j as usize - 1))
}

fn parse_input(text: &str, ty: BaseType) -> Result<Value, String> {
    let t = text.trim();
    match ty {
        BaseType::Int => t.parse().map(Value::Int).map_err(|_| format!("expected an integer, read \"{}\"", text)),
        BaseType::Real => t
            .replace(',', ".")
            .parse()
            .map(Value::Real)
            .map_err(|_| format!("expected a number (like 3.5), read \"{}\"", text)),
        BaseType::Bool => match t.to_uppercase().as_str() {
            "TRUE" => Ok(Value::Bool(true)),
            "FALSE" => Ok(Value::Bool(false)),
            _ => Err(format!("expected TRUE or FALSE, read \"{}\"", text)),
        },
        BaseType::Str => Ok(Value::Str(text.into())),
    }
}
```
Then replace the `Op::RetNone` arm of the `match` in `execute` with these arms (`RetNone` included):
```rust
                Op::GetGlobal(d, g) => {
                    let v = self.globals[g as usize].clone();
                    if let Value::Null = v {
                        fail!(format!("variable \"{}\" has no value yet", m.globals[g as usize]));
                    }
                    reg!(d) = v;
                }
                Op::SetGlobal(g, s) => {
                    let v = reg!(s).clone();
                    self.globals[g as usize] = v;
                }
                Op::NewArray(d, n) => reg!(d) = Value::Seq(Rc::new(RefCell::new(vec![Value::Null; n as usize]))),
                Op::NewMatrix(d, r, c) => {
                    let (rows, cols) = (r as usize, c as usize);
                    let data = vec![Value::Null; rows * cols];
                    reg!(d) = Value::Matrix(Rc::new(RefCell::new(Matrix { rows, cols, data })));
                }
                Op::NewList(d) => reg!(d) = Value::Seq(Rc::new(RefCell::new(Vec::new()))),
                Op::GetElem(d, c, i) => {
                    let i = int(&reg!(i));
                    let v = {
                        let s = seq(&reg!(c)).borrow();
                        match position(i, s.len()) {
                            Ok(k) => s[k].clone(),
                            Err(msg) => fail!(msg),
                        }
                    };
                    if let Value::Null = v {
                        fail!("this element has no value yet");
                    }
                    reg!(d) = v;
                }
                Op::SetElem(c, i, s) => {
                    let i = int(&reg!(i));
                    let v = reg!(s).clone();
                    let mut q = seq(&reg!(c)).borrow_mut();
                    match position(i, q.len()) {
                        Ok(k) => q[k] = v,
                        Err(msg) => fail!(msg),
                    }
                }
                Op::GetElem2(d, c, i, j) => {
                    let (i, j) = (int(&reg!(i)), int(&reg!(j)));
                    let v = {
                        let mx = matrix(&reg!(c)).borrow();
                        match cell(i, j, &mx) {
                            Ok(k) => mx.data[k].clone(),
                            Err(msg) => fail!(msg),
                        }
                    };
                    if let Value::Null = v {
                        fail!("this element has no value yet");
                    }
                    reg!(d) = v;
                }
                Op::SetElem2(c, i, j, s) => {
                    let (i, j) = (int(&reg!(i)), int(&reg!(j)));
                    let v = reg!(s).clone();
                    let mut mx = matrix(&reg!(c)).borrow_mut();
                    match cell(i, j, &mx) {
                        Ok(k) => mx.data[k] = v,
                        Err(msg) => fail!(msg),
                    }
                }
                Op::CharAt(d, s, i) => {
                    let i = int(&reg!(i));
                    let ch = {
                        let text = string(&reg!(s));
                        let n = text.chars().count();
                        match position(i, n) {
                            Ok(k) => text.chars().nth(k).expect("index checked").to_string(),
                            Err(_) => fail!(format!("index {} is out of range: the string has {} characters", i, n)),
                        }
                    };
                    reg!(d) = Value::Str(ch.into());
                }
                Op::Length(d, s) => {
                    let n = match &reg!(s) {
                        Value::Seq(q) => q.borrow().len(),
                        Value::Str(t) => t.chars().count(),
                        other => panic!("internal error: LENGTH of {:?}", other),
                    };
                    reg!(d) = Value::Int(n as i64);
                }
                Op::Append(l, s) => {
                    let v = reg!(s).clone();
                    seq(&reg!(l)).borrow_mut().push(v);
                }
                Op::Remove(l, i) => {
                    let i = int(&reg!(i));
                    let mut q = seq(&reg!(l)).borrow_mut();
                    match position(i, q.len()) {
                        Ok(k) => {
                            q.remove(k);
                        }
                        Err(msg) => fail!(msg),
                    }
                }
                Op::Call { func: f, first, argc, dst } => {
                    if self.frames.len() >= MAX_CALL_DEPTH {
                        fail!("too many nested calls (maybe a recursion that never ends)");
                    }
                    let callee = &m.functions[f as usize];
                    let new_base = self.regs.len();
                    self.regs.resize(new_base + callee.n_regs as usize, Value::Null);
                    for k in 0..argc as usize {
                        let v = self.regs[base + first as usize + k].clone();
                        self.regs[new_base + k] = v;
                    }
                    self.frames.push(Frame { func, ip, base, dst });
                    func = f as usize;
                    code = &callee.code;
                    ip = 0;
                    base = new_base;
                }
                Op::Ret(s) => {
                    let v = std::mem::replace(&mut reg!(s), Value::Null);
                    let Some(fr) = self.frames.pop() else { return Ok(()) };
                    self.regs.truncate(base);
                    func = fr.func;
                    code = &m.functions[func].code;
                    ip = fr.ip;
                    base = fr.base;
                    reg!(fr.dst) = v;
                }
                Op::RetNone => {
                    let Some(fr) = self.frames.pop() else { return Ok(()) };
                    self.regs.truncate(base);
                    func = fr.func;
                    code = &m.functions[func].code;
                    ip = fr.ip;
                    base = fr.base;
                }
                Op::Read(d, ty) => {
                    let _ = output.flush();
                    let mut line = String::new();
                    match input.read_line(&mut line) {
                        Ok(0) | Err(_) => fail!("the input is over: the program expected another value"),
                        Ok(_) => {}
                    }
                    let text = line.trim_end_matches(|c| c == '\n' || c == '\r');
                    match parse_input(text, ty) {
                        Ok(v) => reg!(d) = v,
                        Err(msg) => fail!(msg),
                    }
                }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib vm`
Expected: 19 passed.

- [ ] **Step 6: Commit**

```powershell
git add src/bytecode.rs src/vm.rs
git commit -m "VM: globals, collections, calls and input" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Compiler and library entry points

**Register allocation:**
- Slot k of a function is register k. Temporaries are allocated above the locals and released after each statement.
- A local read used as an operand is used in place, with no copy.
- Assigning to a local compiles the expression straight into the local's register. The exception is `AND`/`OR`, which would overwrite the variable before reading it again, so they go through a temporary.

**Files:**
- Create: `src/compiler.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `tir::*`, `bytecode::*`, `ast::{BinOp, Shape}`.
- Produces: `compiler::compile(&TProgram) -> Module`.
- Produces: `easier::compile(&str) -> Result<Module, Vec<Error>>` and `easier::run(&Module, &mut dyn BufRead, &mut dyn Write) -> Result<(), Error>` (used by Tasks 11–12).

- [ ] **Step 1: Write the library entry points and the failing tests**

Replace `src/lib.rs` with:
```rust
//! Easier: an interpreter for school-style pseudocode.

pub mod ast;
pub mod bytecode;
pub mod checker;
pub mod compiler;
pub mod errors;
pub mod lexer;
pub mod parser;
pub mod tir;
pub mod vm;

use std::io::{BufRead, Write};

pub use errors::Error;

/// Compiles source text to bytecode, or returns every error found.
pub fn compile(source: &str) -> Result<bytecode::Module, Vec<Error>> {
    let tokens = lexer::tokenize(source).map_err(|e| vec![e])?;
    let program = parser::parse(tokens).map_err(|e| vec![e])?;
    let typed = checker::check(&program)?;
    Ok(compiler::compile(&typed))
}

/// Runs a compiled program, reading `input()` lines from `input`.
pub fn run(module: &bytecode::Module, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
    vm::run(module, input, output)
}
```
Create `src/compiler.rs` containing only:
```rust
#[cfg(test)]
mod tests {
    /// Compiles and runs `src`; a run-time error is appended to the output, rendered.
    fn run_src(src: &str, input: &str) -> String {
        let module = crate::compile(src)
            .unwrap_or_else(|errs| panic!("{}", errs.iter().map(|e| e.render(src)).collect::<String>()));
        let mut out = Vec::new();
        let result = crate::run(&module, &mut input.as_bytes(), &mut out);
        let mut text = String::from_utf8(out).unwrap();
        if let Err(e) = result {
            text.push_str(&e.render(src));
        }
        text
    }

    #[test]
    fn arithmetic_and_output() {
        let src = "variables: a, b\na <- 7\nb <- 2\noutput(a + b, \" \", a - b, \" \", a * b, \" \", a DIV b, \" \", a MOD b, \" \", a / b)\n";
        assert_eq!(run_src(src, ""), "9 5 14 3 1 3.5\n");
    }

    #[test]
    fn if_else_chain() {
        let src = "variables: v\ninput(v)\nIF(v >= 8)\n    output(\"great\")\nELSE\nIF(v >= 6)\n    output(\"pass\")\nELSE\n    output(\"fail\")\nEND IF\n";
        assert_eq!(run_src(src, "9\n"), "great\n");
        assert_eq!(run_src(src, "6\n"), "pass\n");
        assert_eq!(run_src(src, "2\n"), "fail\n");
    }

    #[test]
    fn while_and_repeat() {
        let src = "variables: i, n\ni <- 1\nWHILE(i <= 3)\n    output(i)\n    i <- i + 1\nEND LOOP\nREPEAT\n    input(n)\nWHILE(n < 0)\noutput(\"ok \", n)\n";
        assert_eq!(run_src(src, "-1\n-5\n4\n"), "1\n2\n3\nok 4\n");
    }

    #[test]
    fn for_loops() {
        let src = "variables: i\nFOR(i FROM 10 TO 0 STEP -5)\n    output(i)\nEND LOOP\noutput(\"after: \", i)\n";
        assert_eq!(run_src(src, ""), "10\n5\n0\nafter: -5\n");
        let global = "GLOBAL variables: g\nvariables: s\ns <- 0\nFOR(g FROM 1 TO 4)\n    s <- s + g\nEND LOOP\noutput(s, \" \", g)\n";
        assert_eq!(run_src(global, ""), "10 5\n");
    }

    #[test]
    fn functions_and_recursion() {
        let src = "variables: r\nFUNCTION fib(n)\n    IF(n < 2)\n        RETURN n\n    END IF\n    RETURN fib(n - 1) + fib(n - 2)\nEND FUNCTION\nr <- fib(20)\noutput(r)\n";
        assert_eq!(run_src(src, ""), "6765\n");
    }

    #[test]
    fn arrays_by_reference_scalars_by_value() {
        let src = "variables: v[3], i, x\nPROCEDURE fill(a, y)\n    variables: k\n    FOR(k FROM 1 TO 3)\n        a[k] <- k * k\n    END LOOP\n    y <- 100\nEND PROCEDURE\nx <- 1\nfill(v, x)\nFOR(i FROM 1 TO 3)\n    output(v[i])\nEND LOOP\noutput(x)\n";
        assert_eq!(run_src(src, ""), "1\n4\n9\n1\n");
    }

    #[test]
    fn lists_strings_and_matrices() {
        let src = "variables: l[], s, m[2, 3], r, c, t\nAPPEND(l, \"a\")\nAPPEND(l, \"b\")\nAPPEND(l, \"c\")\nREMOVE(l, 2)\noutput(LENGTH(l), \" \", l[1], l[2])\ns <- \"ciao\"\noutput(s[2], LENGTH(s))\nt <- 0\nFOR(r FROM 1 TO 2)\n    FOR(c FROM 1 TO 3)\n        m[r, c] <- r * c\n        t <- t + m[r, c]\n    END LOOP\nEND LOOP\noutput(t)\n";
        assert_eq!(run_src(src, ""), "2 ac\ni4\n18\n");
    }

    #[test]
    fn short_circuit_and_logic_into_variable() {
        let src = "variables: n, b, c\nn <- 0\nIF(n != 0 AND 10 DIV n > 1)\n    output(\"no\")\nELSE\n    output(\"safe\")\nEND IF\nb <- FALSE\nc <- TRUE\nb <- c AND b\noutput(b)\n";
        assert_eq!(run_src(src, ""), "safe\nFALSE\n");
    }

    #[test]
    fn reals_and_conversions() {
        let src = "variables: x\nx <- 1\nx <- x / 4\noutput(x, \" \", 2 * 1.5, \" \", \"n=\" + 3 + TRUE)\n";
        assert_eq!(run_src(src, ""), "0.25 3.0 n=3TRUE\n");
    }

    #[test]
    fn runtime_errors_point_at_the_code() {
        assert_eq!(
            run_src("variables: v[3]\nv[4] <- 1\n", ""),
            "Error at line 2, column 1: index 4 is out of range: valid positions are 1 to 3\n    v[4] <- 1\n    ^\n"
        );
        assert_eq!(
            run_src("variables: x\nx <- 0\noutput(1 / x)\n", ""),
            "Error at line 3, column 10: division by zero\n    output(1 / x)\n             ^\n"
        );
        let global = run_src("GLOBAL variables: g\nPROCEDURE show()\n    output(g)\nEND PROCEDURE\nshow()\ng <- 1\n", "");
        assert!(global.starts_with("Error at line 3, column 12: variable \"g\" has no value yet"), "{}", global);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib compiler`
Expected: compile error, `cannot find function compile in module compiler`.

- [ ] **Step 3: Implement**

Add above the test module in `src/compiler.rs`:
```rust
//! Translates the typed program into register bytecode.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{BinOp, Shape};
use crate::bytecode::{Cmp, Function, Module, Op, Reg};
use crate::errors::Pos;
use crate::tir::*;

pub fn compile(p: &TProgram) -> Module {
    let mut strings = Strings::default();
    let functions = p
        .functions
        .iter()
        .enumerate()
        .map(|(k, f)| {
            let globals = if k == 0 { Some(&p.globals[..]) } else { None };
            FnCompiler::new(f, globals, &mut strings).finish()
        })
        .collect();
    Module { functions, strings: strings.list, globals: p.globals.iter().map(|s| s.name.clone()).collect() }
}

#[derive(Default)]
struct Strings {
    list: Vec<Rc<str>>,
    index: HashMap<Rc<str>, u32>,
}

impl Strings {
    fn intern(&mut self, s: &Rc<str>) -> u32 {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(s.clone());
        self.index.insert(s.clone(), i);
        i
    }
}

fn alloc(shape: &Shape, dst: Reg) -> Option<Op> {
    match *shape {
        Shape::Array(n) => Some(Op::NewArray(dst, n as u32)),
        Shape::Matrix(r, c) => Some(Op::NewMatrix(dst, r as u32, c as u32)),
        Shape::List => Some(Op::NewList(dst)),
        Shape::Scalar => None,
    }
}

fn binary_op(op: BinOp, t: BaseType, d: Reg, a: Reg, b: Reg) -> Op {
    let cmp = |c: Cmp| match t {
        BaseType::Int => Op::CmpI(c, d, a, b),
        BaseType::Real => Op::CmpR(c, d, a, b),
        BaseType::Str => Op::CmpS(c, d, a, b),
        BaseType::Bool => Op::CmpB(c, d, a, b),
    };
    match (op, t) {
        (BinOp::Add, BaseType::Int) => Op::AddI(d, a, b),
        (BinOp::Add, BaseType::Real) => Op::AddR(d, a, b),
        (BinOp::Add, BaseType::Str) => Op::Concat(d, a, b),
        (BinOp::Sub, BaseType::Int) => Op::SubI(d, a, b),
        (BinOp::Sub, BaseType::Real) => Op::SubR(d, a, b),
        (BinOp::Mul, BaseType::Int) => Op::MulI(d, a, b),
        (BinOp::Mul, BaseType::Real) => Op::MulR(d, a, b),
        (BinOp::Div, _) => Op::DivR(d, a, b),
        (BinOp::IntDiv, _) => Op::DivI(d, a, b),
        (BinOp::Mod, _) => Op::ModI(d, a, b),
        (BinOp::Eq, _) => cmp(Cmp::Eq),
        (BinOp::NotEq, _) => cmp(Cmp::NotEq),
        (BinOp::Lt, _) => cmp(Cmp::Lt),
        (BinOp::LtEq, _) => cmp(Cmp::LtEq),
        (BinOp::Gt, _) => cmp(Cmp::Gt),
        (BinOp::GtEq, _) => cmp(Cmp::GtEq),
        (op, t) => panic!("internal error: no instruction for {:?} on {:?}", op, t),
    }
}

struct FnCompiler<'a> {
    f: &'a TFunction,
    /// only for the main program, which allocates global collections
    globals: Option<&'a [Slot]>,
    strings: &'a mut Strings,
    code: Vec<Op>,
    positions: Vec<Pos>,
    n_locals: Reg,
    next: Reg,
    max: Reg,
}

impl<'a> FnCompiler<'a> {
    fn new(f: &'a TFunction, globals: Option<&'a [Slot]>, strings: &'a mut Strings) -> FnCompiler<'a> {
        let n = f.locals.len() as Reg;
        FnCompiler { f, globals, strings, code: Vec::new(), positions: Vec::new(), n_locals: n, next: n, max: n }
    }

    fn finish(mut self) -> Function {
        let start = Pos::default();
        if let Some(globals) = self.globals {
            for (g, slot) in globals.iter().enumerate() {
                let t = self.temp();
                if let Some(op) = alloc(&slot.shape, t) {
                    self.emit(op, start);
                    self.emit(Op::SetGlobal(g as u16, t), start);
                }
                self.next -= 1;
            }
        }
        let f = self.f;
        for (k, slot) in f.locals.iter().enumerate() {
            if !slot.is_param {
                if let Some(op) = alloc(&slot.shape, k as Reg) {
                    self.emit(op, start);
                }
            }
        }
        self.block(&f.body);
        self.emit(Op::RetNone, start);
        Function {
            name: f.name.clone(),
            n_params: f.n_params as u16,
            n_regs: self.max,
            code: self.code,
            positions: self.positions,
        }
    }

    fn emit(&mut self, op: Op, pos: Pos) -> usize {
        self.code.push(op);
        self.positions.push(pos);
        self.code.len() - 1
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    fn temp(&mut self) -> Reg {
        let r = self.next;
        self.next += 1;
        self.max = self.max.max(self.next);
        r
    }

    fn patch(&mut self, at: usize, target: u32) {
        match &mut self.code[at] {
            Op::Jump(t) | Op::JumpIfFalse(_, t) | Op::JumpIfTrue(_, t) => *t = target,
            Op::ForInit { exit, .. } => *exit = target,
            other => panic!("internal error: cannot patch {:?}", other),
        }
    }

    /// Evaluates `e` and returns the register holding the result: the
    /// variable's own register for a local read, otherwise a new temporary.
    fn operand(&mut self, e: &TExpr) -> Reg {
        if let TExprKind::Load(Place::Local(r)) = &e.kind {
            return *r;
        }
        let t = self.temp();
        self.expr_into(e, t);
        t
    }

    fn collection(&mut self, p: Place, pos: Pos) -> Reg {
        match p {
            Place::Local(r) => r,
            Place::Global(g) => {
                let t = self.temp();
                self.emit(Op::GetGlobal(t, g), pos);
                t
            }
        }
    }

    /// Evaluates call arguments into consecutive fresh registers; returns the first.
    fn args(&mut self, args: &[TExpr]) -> Reg {
        let first = self.next;
        for a in args {
            let t = self.temp();
            self.expr_into(a, t);
        }
        first
    }

    fn expr_into(&mut self, e: &TExpr, d: Reg) {
        let mark = self.next;
        let pos = e.pos;
        match &e.kind {
            TExprKind::Int(n) => {
                self.emit(Op::LoadInt(d, *n), pos);
            }
            TExprKind::Real(r) => {
                self.emit(Op::LoadReal(d, *r), pos);
            }
            TExprKind::Bool(b) => {
                self.emit(Op::LoadBool(d, *b), pos);
            }
            TExprKind::Str(s) => {
                let i = self.strings.intern(s);
                self.emit(Op::LoadStr(d, i), pos);
            }
            TExprKind::Load(Place::Local(r)) => {
                if *r != d {
                    self.emit(Op::Move(d, *r), pos);
                }
            }
            TExprKind::Load(Place::Global(g)) => {
                self.emit(Op::GetGlobal(d, *g), pos);
            }
            TExprKind::Elem(place, idx) => {
                let c = self.collection(*place, pos);
                let i = self.operand(&idx[0]);
                if idx.len() == 2 {
                    let j = self.operand(&idx[1]);
                    self.emit(Op::GetElem2(d, c, i, j), pos);
                } else {
                    self.emit(Op::GetElem(d, c, i), pos);
                }
            }
            TExprKind::Char(s, i) => {
                let s = self.operand(s);
                let i = self.operand(i);
                self.emit(Op::CharAt(d, s, i), pos);
            }
            TExprKind::ToReal(x) => {
                let r = self.operand(x);
                self.emit(Op::ToReal(d, r), pos);
            }
            TExprKind::ToStr(x) => {
                let r = self.operand(x);
                self.emit(Op::ToStr(d, r), pos);
            }
            TExprKind::Binary(op @ (BinOp::And | BinOp::Or), _, a, b) => self.logic(*op, a, b, d, pos),
            TExprKind::Binary(op, t, a, b) => {
                let x = self.operand(a);
                let y = self.operand(b);
                self.emit(binary_op(*op, *t, d, x, y), pos);
            }
            TExprKind::Neg(t, x) => {
                let r = self.operand(x);
                self.emit(if *t == BaseType::Int { Op::NegI(d, r) } else { Op::NegR(d, r) }, pos);
            }
            TExprKind::Not(x) => {
                let r = self.operand(x);
                self.emit(Op::Not(d, r), pos);
            }
            TExprKind::Call(f, args) => {
                let first = self.args(args);
                self.emit(Op::Call { func: *f as u32, first, argc: args.len() as u16, dst: d }, pos);
            }
            TExprKind::Length(x) => {
                let r = self.operand(x);
                self.emit(Op::Length(d, r), pos);
            }
        }
        self.next = mark;
    }

    fn logic(&mut self, op: BinOp, a: &TExpr, b: &TExpr, d: Reg, pos: Pos) {
        // writing straight into a variable would overwrite it before `b` reads it
        let r = if d < self.n_locals { self.temp() } else { d };
        self.expr_into(a, r);
        let jump = self.emit(if op == BinOp::And { Op::JumpIfFalse(r, 0) } else { Op::JumpIfTrue(r, 0) }, pos);
        self.expr_into(b, r);
        let end = self.here();
        self.patch(jump, end);
        if r != d {
            self.emit(Op::Move(d, r), pos);
        }
    }

    fn block(&mut self, body: &[TStmt]) {
        for s in body {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &TStmt) {
        let mark = self.next;
        match s {
            TStmt::Assign(t, v) => self.assign(t, v),
            TStmt::If(branches, other) => {
                let mut exits = Vec::new();
                for (c, body) in branches {
                    let r = self.operand(c);
                    let skip = self.emit(Op::JumpIfFalse(r, 0), c.pos);
                    self.next = mark;
                    self.block(body);
                    exits.push(self.emit(Op::Jump(0), c.pos));
                    let here = self.here();
                    self.patch(skip, here);
                }
                if let Some(body) = other {
                    self.block(body);
                }
                let end = self.here();
                for e in exits {
                    self.patch(e, end);
                }
            }
            TStmt::While(c, body) => {
                let top = self.here();
                let r = self.operand(c);
                let exit = self.emit(Op::JumpIfFalse(r, 0), c.pos);
                self.next = mark;
                self.block(body);
                self.emit(Op::Jump(top), c.pos);
                let end = self.here();
                self.patch(exit, end);
            }
            TStmt::Repeat(body, c) => {
                let top = self.here();
                self.block(body);
                let r = self.operand(c);
                self.emit(Op::JumpIfTrue(r, top), c.pos);
            }
            TStmt::For { var, from, to, step, body, pos } => self.for_loop(*var, from, to, step.as_ref(), body, *pos),
            TStmt::Call(f, args, pos) => {
                let first = self.args(args);
                let dst = self.temp();
                self.emit(Op::Call { func: *f as u32, first, argc: args.len() as u16, dst }, *pos);
            }
            TStmt::Output(args) => {
                for a in args {
                    let r = self.operand(a);
                    self.emit(Op::Print(r), a.pos);
                    self.next = mark;
                }
                self.emit(Op::PrintLn, Pos::default());
            }
            TStmt::Input(targets) => {
                for (t, ty) in targets {
                    match (t.place, t.indices.is_empty()) {
                        (Place::Local(r), true) => {
                            self.emit(Op::Read(r, *ty), t.pos);
                        }
                        _ => {
                            let r = self.temp();
                            self.emit(Op::Read(r, *ty), t.pos);
                            self.store(t, r);
                        }
                    }
                    self.next = mark;
                }
            }
            TStmt::Append(list, v) => {
                let l = self.operand(list);
                let r = self.operand(v);
                self.emit(Op::Append(l, r), list.pos);
            }
            TStmt::Remove(list, i, pos) => {
                let l = self.operand(list);
                let r = self.operand(i);
                self.emit(Op::Remove(l, r), *pos);
            }
            TStmt::Return(Some(v), pos) => {
                let r = self.operand(v);
                self.emit(Op::Ret(r), *pos);
            }
            TStmt::Return(None, pos) => {
                self.emit(Op::RetNone, *pos);
            }
        }
        self.next = mark;
    }

    fn assign(&mut self, t: &TTarget, v: &TExpr) {
        match (t.place, t.indices.is_empty()) {
            (Place::Local(r), true) => self.expr_into(v, r),
            _ => {
                let r = self.operand(v);
                self.store(t, r);
            }
        }
    }

    fn store(&mut self, t: &TTarget, value: Reg) {
        match t.indices.len() {
            0 => match t.place {
                Place::Global(g) => {
                    self.emit(Op::SetGlobal(g, value), t.pos);
                }
                Place::Local(r) => {
                    if r != value {
                        self.emit(Op::Move(r, value), t.pos);
                    }
                }
            },
            1 => {
                let c = self.collection(t.place, t.pos);
                let i = self.operand(&t.indices[0]);
                self.emit(Op::SetElem(c, i, value), t.pos);
            }
            _ => {
                let c = self.collection(t.place, t.pos);
                let i = self.operand(&t.indices[0]);
                let j = self.operand(&t.indices[1]);
                self.emit(Op::SetElem2(c, i, j, value), t.pos);
            }
        }
    }

    fn for_loop(&mut self, var: Place, from: &TExpr, to: &TExpr, step: Option<&TExpr>, body: &[TStmt], pos: Pos) {
        // bounds and step are evaluated once, before the loop variable changes
        let start = self.temp();
        self.expr_into(from, start);
        let end = self.temp();
        self.expr_into(to, end);
        let step_reg = self.temp();
        match step {
            Some(s) => self.expr_into(s, step_reg),
            None => {
                self.emit(Op::LoadInt(step_reg, 1), pos);
            }
        }
        let counter = match var {
            Place::Local(r) => r,
            Place::Global(_) => self.temp(),
        };
        self.emit(Op::Move(counter, start), pos);
        let init = self.emit(Op::ForInit { var: counter, end, step: step_reg, exit: 0 }, pos);
        let top = self.here();
        if let Place::Global(g) = var {
            self.emit(Op::SetGlobal(g, counter), pos);
        }
        self.block(body);
        self.emit(Op::ForStep { var: counter, end, step: step_reg, body: top }, pos);
        let exit = self.here();
        self.patch(init, exit);
        if let Place::Global(g) = var {
            // like a local counter, the global ends at the first value that failed the test
            self.emit(Op::SetGlobal(g, counter), pos);
        }
    }
}
```

- [ ] **Step 4: Run all the tests**

Run: `cargo test --lib`
Expected: every test passes (compiler: 10).

- [ ] **Step 5: Commit**

```powershell
git add src/lib.rs src/compiler.rs
git commit -m "Add compiler and library entry points" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Command line and example programs

**Files:**
- Create: `src/main.rs`, `tests/examples.rs`, `tests/cli.rs`, `examples/*.ez`, `examples/*.input`, `examples/*.expected`

**Interfaces:**
- Consumes: `easier::{compile, run, Error}`.
- Produces: the `easier` binary.
  - `easier program.ez` runs the program.
  - `easier --check program.ez` prints `No errors found.` on success.
  - Errors go to stderr, rendered, separated by a blank line. Exit codes are 0/1/2/3.

- [ ] **Step 1: Write the example programs**

Every `.expected` file ends with a newline. Write each file exactly as shown.

`examples/hello.ez`:
```
output("Hello, world!")
```
`examples/hello.expected`:
```
Hello, world!
```

`examples/sum_to_n.ez`:
```
' Sum of the numbers from 1 to n
variables: n, i, total
input(n)
total <- 0
FOR(i FROM 1 TO n)
    total <- total + i
END LOOP
output("The sum is ", total)
```
`examples/sum_to_n.input`:
```
5
```
`examples/sum_to_n.expected`:
```
The sum is 15
```

`examples/even_odd.ez`:
```
variables: n, i
input(n)
FOR(i FROM 1 TO n)
    IF(i MOD 2 = 0)
        output(i, " is even")
    ELSE
        output(i, " is odd")
    END IF
END LOOP
```
`examples/even_odd.input`:
```
4
```
`examples/even_odd.expected`:
```
1 is odd
2 is even
3 is odd
4 is even
```

`examples/grades.ez`:
```
variables: grades[5], i, total, average
FOR(i FROM 1 TO 5)
    input(grades[i])
END LOOP
total <- 0
FOR(i FROM 1 TO 5)
    total <- total + grades[i]
END LOOP
average <- total / 5
output("Average: ", average)
IF(average >= 8)
    output("Great")
ELSE
IF(average >= 6)
    output("Pass")
ELSE
    output("Fail")
END IF
```
`examples/grades.input`:
```
7
8
6
9
5
```
`examples/grades.expected`:
```
Average: 7.0
Pass
```

`examples/factorial.ez`:
```
variables: n
FUNCTION factorial(k)
    IF(k <= 1)
        RETURN 1
    END IF
    RETURN k * factorial(k - 1)
END FUNCTION
FOR(n FROM 1 TO 5)
    output(n, "! = ", factorial(n))
END LOOP
```
`examples/factorial.expected`:
```
1! = 1
2! = 2
3! = 6
4! = 24
5! = 120
```

`examples/shopping_list.ez`:
```
variables: items[], i
APPEND(items, "bread")
APPEND(items, "milk")
APPEND(items, "eggs")
REMOVE(items, 2)
output("Items: ", LENGTH(items))
FOR(i FROM 1 TO LENGTH(items))
    output("- ", items[i])
END LOOP
```
`examples/shopping_list.expected`:
```
Items: 2
- bread
- eggs
```

`examples/times_table.ez`:
```
variables: t[3, 3], r, c, line
FOR(r FROM 1 TO 3)
    FOR(c FROM 1 TO 3)
        t[r, c] <- r * c
    END LOOP
END LOOP
FOR(r FROM 1 TO 3)
    line <- ""
    FOR(c FROM 1 TO 3)
        line <- line + t[r, c] + " "
    END LOOP
    output(line)
END LOOP
```
`examples/times_table.expected` (each line ends with one space before the newline):
```
1 2 3 
2 4 6 
3 6 9 
```

`examples/counter_global.ez`:
```
GLOBAL variables: count
PROCEDURE tick()
    count <- count + 1
END PROCEDURE
count <- 0
tick()
tick()
tick()
output("count = ", count)
```
`examples/counter_global.expected`:
```
count = 3
```

`examples/repeat_input.ez`:
```
variables: n
REPEAT
    output("Enter a positive number:")
    input(n)
WHILE(n <= 0)
output("Thanks, ", n)
```
`examples/repeat_input.input`:
```
-3
0
7
```
`examples/repeat_input.expected`:
```
Enter a positive number:
Enter a positive number:
Enter a positive number:
Thanks, 7
```

`examples/error_undeclared.ez`:
```
variables: total
total <- 0
count <- total + 1
```
`examples/error_undeclared.expected`:
```
Error at line 3, column 1: variable "count" is not declared
    count <- total + 1
    ^^^^^
Hint: add it to "variables:" at the top of the program
```

`examples/error_lowercase.ez`:
```
variables: x
x <- 1
if(x > 0)
    output(x)
END IF
```
`examples/error_lowercase.expected`:
```
Error at line 3, column 1: "if" is not a keyword
    if(x > 0)
    ^^
Hint: keywords are written in UPPERCASE: write IF
```

`examples/error_index.ez`:
```
variables: v[3], i
FOR(i FROM 1 TO 4)
    v[i] <- i
END LOOP
```
`examples/error_index.expected`:
```
Error at line 3, column 5: index 4 is out of range: valid positions are 1 to 3
        v[i] <- i
        ^
```

- [ ] **Step 2: Write the failing integration tests**

`tests/examples.rs`:
```rust
use std::fs;
use std::path::Path;

/// Output of an example as a user would see it: program output followed by
/// the rendered error, if any.
fn run_example(path: &Path) -> String {
    let src = fs::read_to_string(path).unwrap();
    let input = fs::read_to_string(path.with_extension("input")).unwrap_or_default();
    match easier::compile(&src) {
        Err(errors) => errors.iter().map(|e| e.render(&src)).collect::<Vec<_>>().join("\n"),
        Ok(module) => {
            let mut out = Vec::new();
            let result = easier::run(&module, &mut input.as_bytes(), &mut out);
            let mut text = String::from_utf8(out).unwrap();
            if let Err(e) = result {
                text.push_str(&e.render(&src));
            }
            text
        }
    }
}

#[test]
fn all_examples() {
    let mut failures = Vec::new();
    let mut count = 0;
    for entry in fs::read_dir("examples").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(true, |e| e != "ez") {
            continue;
        }
        count += 1;
        let expected_path = path.with_extension("expected");
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|_| panic!("missing {}", expected_path.display()))
            .replace("\r\n", "\n");
        let actual = run_example(&path).replace("\r\n", "\n");
        if actual != expected {
            failures.push(format!("== {} ==\n--- expected ---\n{}--- actual ---\n{}", path.display(), expected, actual));
        }
    }
    assert!(count >= 12, "found only {} examples", count);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
```

`tests/cli.rs`:
```rust
use std::process::Command;

fn easier(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_easier")).args(args).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

#[test]
fn runs_a_program() {
    let (code, stdout, _) = easier(&["examples/hello.ez"]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Hello, world!\n");
}

#[test]
fn check_mode() {
    let (code, stdout, _) = easier(&["--check", "examples/factorial.ez"]);
    assert_eq!((code, stdout.as_str()), (0, "No errors found.\n"));
    let (code, _, stderr) = easier(&["--check", "examples/error_undeclared.ez"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("variable \"count\" is not declared"), "{}", stderr);
}

#[test]
fn runtime_error_exit_code() {
    let (code, _, stderr) = easier(&["examples/error_index.ez"]);
    assert_eq!(code, 2);
    assert!(stderr.starts_with("Error at line 3"), "{}", stderr);
}

#[test]
fn usage_errors() {
    let (code, _, stderr) = easier(&[]);
    assert_eq!(code, 3);
    assert!(stderr.starts_with("Usage:"), "{}", stderr);
    let (code, _, stderr) = easier(&["missing.ez"]);
    assert_eq!(code, 3);
    assert!(stderr.starts_with("cannot read \"missing.ez\""), "{}", stderr);
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --test examples --test cli`
Expected: `examples` may already pass, since it uses the library. `cli` fails to compile with `environment variable CARGO_BIN_EXE_easier not defined` because there is no binary yet.

- [ ] **Step 4: Implement the CLI**

`src/main.rs`:
```rust
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (check_only, file) = match args.as_slice() {
        [f] if f != "--check" => (false, f),
        [flag, f] if flag == "--check" => (true, f),
        _ => {
            eprintln!("Usage: easier program.ez\n       easier --check program.ez");
            return ExitCode::from(3);
        }
    };
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read \"{}\": {}", file, e);
            return ExitCode::from(3);
        }
    };
    let module = match easier::compile(&source) {
        Ok(m) => m,
        Err(errors) => {
            let text: Vec<String> = errors.iter().map(|e| e.render(&source)).collect();
            eprint!("{}", text.join("\n"));
            return ExitCode::from(1);
        }
    };
    if check_only {
        println!("No errors found.");
        return ExitCode::SUCCESS;
    }
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    let result = easier::run(&module, &mut input, &mut output);
    let _ = output.flush();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprint!("{}", e.render(&source));
            ExitCode::from(2)
        }
    }
}
```

- [ ] **Step 5: Run the whole test suite**

Run: `cargo test`
Expected: all unit tests plus `examples` (1) and `cli` (4) pass. If an example fails, read the diff in the failure message. Fix the code, not the `.expected` file, unless the expected file contradicts `docs/spec.md`.

- [ ] **Step 6: Try it by hand**

Run: `cargo run --release -- examples/grades.ez`, then type `7`, `8`, `6`, `9`, `5` (Enter after each).
Expected: `Average: 7.0` then `Pass`.

- [ ] **Step 7: Commit**

```powershell
git add src/main.rs tests examples
git commit -m "Add command line and example programs" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Benchmarks and performance target

**Files:**
- Create: `bench/fib.ez`, `bench/fib.py`, `bench/sieve.ez`, `bench/sieve.py`, `bench/sum.ez`, `bench/sum.py`, `bench/run.ps1`
- Modify (only if the target is missed): `src/bytecode.rs`, `src/vm.rs`, `src/compiler.rs`

**Interfaces:**
- Consumes: the release binary `target/release/easier.exe`.
- Produces: `bench/run.ps1`, which prints one line per benchmark with both times and the speed-up.

- [ ] **Step 1: Write the benchmark programs**

`bench/fib.ez`:
```
variables: r
FUNCTION fib(n)
    IF(n < 2)
        RETURN n
    END IF
    RETURN fib(n - 1) + fib(n - 2)
END FUNCTION
r <- fib(30)
output(r)
```
`bench/fib.py`:
```python
def fib(n):
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)


print(fib(30))
```
`bench/sieve.ez`:
```
variables: prime[10000000], i, j, n, count
n <- 10000000
FOR(i FROM 2 TO n)
    prime[i] <- TRUE
END LOOP
prime[1] <- FALSE
FOR(i FROM 2 TO n)
    IF(prime[i])
        j <- i * i
        WHILE(j <= n)
            prime[j] <- FALSE
            j <- j + i
        END LOOP
    END IF
END LOOP
count <- 0
FOR(i FROM 1 TO n)
    IF(prime[i])
        count <- count + 1
    END IF
END LOOP
output(count)
```
`bench/sieve.py`:
```python
def main():
    n = 10_000_000
    prime = [False] * (n + 1)
    for i in range(2, n + 1):
        prime[i] = True
    for i in range(2, n + 1):
        if prime[i]:
            j = i * i
            while j <= n:
                prime[j] = False
                j += i
    count = 0
    for i in range(1, n + 1):
        if prime[i]:
            count += 1
    print(count)


main()
```
`bench/sum.ez`:
```
variables: i, s
s <- 0
FOR(i FROM 1 TO 100000000)
    s <- s + i MOD 7
END LOOP
output(s)
```
`bench/sum.py`:
```python
def main():
    s = 0
    for i in range(1, 100_000_001):
        s += i % 7
    print(s)


main()
```
`bench/run.ps1`:
```powershell
# Compares Easier with CPython on the benchmark programs. Run from anywhere:
#   powershell -File bench\run.ps1
param([string]$Python = "python")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$easier = Join-Path $root "target\release\easier.exe"
if (-not (Test-Path $easier)) { throw "build first: cargo build --release" }
foreach ($name in "fib", "sieve", "sum") {
    $ez = Join-Path $PSScriptRoot "$name.ez"
    $py = Join-Path $PSScriptRoot "$name.py"
    $outEz = & $easier $ez
    $outPy = & $Python $py
    if ("$outEz" -ne "$outPy") { throw "$name: easier printed '$outEz', python printed '$outPy'" }
    $tEz = (Measure-Command { & $easier $ez | Out-Null }).TotalSeconds
    $tPy = (Measure-Command { & $Python $py | Out-Null }).TotalSeconds
    "{0,-6} easier {1,7:N2}s   python {2,7:N2}s   {3,5:N1}x faster" -f $name, $tEz, $tPy, ($tPy / $tEz)
}
```

- [ ] **Step 2: Build and measure**

Run: `cargo build --release; powershell -File bench\run.ps1`
Expected: three lines, and the outputs agree (fib 832040, sieve 664579, sum 299999997). Record the three speed-ups in the commit message.

- [ ] **Step 3: If any speed-up is below 5×, fuse compare-and-jump**

Skip this step if all three are ≥ 5×. Otherwise:

Add to `enum Op` in `src/bytecode.rs`:
```rust
    /// jumps to the target when the integer comparison a <cmp> b is false
    JumpUnlessI(Cmp, Reg, Reg, u32),
```
Add the arm to the `match` in `src/vm.rs`:
```rust
                Op::JumpUnlessI(c, a, b, t) => {
                    if !compare(c, &int(&reg!(a)), &int(&reg!(b))) {
                        ip = t as usize;
                    }
                }
```
In `src/compiler.rs`, add `Op::JumpUnlessI(_, _, _, t)` to the first arm of `patch`, and add these helpers:
```rust
fn cmp_of(op: BinOp) -> Option<Cmp> {
    Some(match op {
        BinOp::Eq => Cmp::Eq,
        BinOp::NotEq => Cmp::NotEq,
        BinOp::Lt => Cmp::Lt,
        BinOp::LtEq => Cmp::LtEq,
        BinOp::Gt => Cmp::Gt,
        BinOp::GtEq => Cmp::GtEq,
        _ => return None,
    })
}
```
```rust
    /// Emits a jump taken when `c` is false; returns its index for patching.
    fn jump_unless(&mut self, c: &TExpr) -> usize {
        if let TExprKind::Binary(op, BaseType::Int, a, b) = &c.kind {
            if let Some(cmp) = cmp_of(*op) {
                let x = self.operand(a);
                let y = self.operand(b);
                return self.emit(Op::JumpUnlessI(cmp, x, y, 0), c.pos);
            }
        }
        let r = self.operand(c);
        self.emit(Op::JumpIfFalse(r, 0), c.pos)
    }
```
In `stmt`, in the `If` arm replace
```rust
                    let r = self.operand(c);
                    let skip = self.emit(Op::JumpIfFalse(r, 0), c.pos);
```
with `let skip = self.jump_unless(c);`. In the `While` arm replace
```rust
                let r = self.operand(c);
                let exit = self.emit(Op::JumpIfFalse(r, 0), c.pos);
```
with `let exit = self.jump_unless(c);`.

Run `cargo test`. Expected: all pass. Then rebuild and re-measure as in Step 2. If a benchmark is still below 5×, stop and report the numbers to your human partner instead of guessing further optimizations.

- [ ] **Step 4: Commit**

```powershell
git add bench src
git commit -m "Add benchmarks against CPython" -m "fib: <N>x, sieve: <N>x, sum: <N>x" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
(Replace `<N>` with the measured speed-ups.)
