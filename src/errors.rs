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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_line_caret_and_hint() {
        let src = "variables: a\n    total = 0\n";
        let e = Error::new(Pos::new(2, 5, 5), "variable \"total\" is not declared")
            .with_hint("add it to \"variables:\" at the top of the program");
        let expected = concat!(
            "Error at line 2, column 5: variable \"total\" is not declared\n",
            "        total = 0\n",
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
