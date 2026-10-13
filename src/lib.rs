//! Easier: an interpreter for school-style pseudocode.

pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod canvas;
pub mod checker;
pub mod compiler;
pub mod dialogs;
pub mod errors;
pub mod graphics;
pub mod lexer;
pub mod parser;
pub mod render3d;
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
