//! Globw: an interpreter for school-style pseudocode.

pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod canvas;
pub mod checker;
pub mod compiler;
pub mod dialogs;
pub mod errors;
pub mod fonts;
pub mod graphics;
pub mod lexer;
pub mod classes;
pub mod library;
pub mod modules;
pub mod parser;
#[cfg(feature = "native")]
pub mod pkg;
pub mod render3d;
pub mod tir;
pub mod vm;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use std::io::{BufRead, Write};

pub use errors::{Error, SourceFile};

/// Compiles source text to bytecode, or returns every error found. Imported
/// modules are looked for in the current folder.
pub fn compile(source: &str) -> Result<bytecode::Module, Vec<Error>> {
    let main = SourceFile { name: "program.gw".to_string(), text: source.to_string() };
    compile_program(main, std::path::Path::new(".")).1
}

/// Compiles a program and the modules it imports (looked for in `dir`, the
/// folder of the program). The files come back too, for rendering errors.
pub fn compile_program(
    main: SourceFile,
    dir: &std::path::Path,
) -> (Vec<SourceFile>, Result<bytecode::Module, Vec<Error>>) {
    let (files, program) = modules::load(main, dir);
    let result = program
        .and_then(|mut p| classes::lower(&mut p).map(|()| p))
        .and_then(|p| checker::check(&p))
        .map(|t| compiler::compile(&t));
    (files, result)
}

/// Runs a compiled program with opcode profiling; returns the counts.
pub fn run_profiled(
    module: &bytecode::Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Vec<(&'static str, u64)>, Error> {
    vm::run_profiled(module, input, output)
}

/// Runs a compiled program, reading `input()` lines from `input`.
pub fn run(module: &bytecode::Module, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
    vm::run(module, input, output)
}
