//! Globw: an interpreter for school-style pseudocode.

pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod canvas;
pub mod checker;
pub mod compiler;
pub mod debug;
pub mod dialogs;
pub mod errors;
pub mod fonts;
pub mod graphics;
pub mod gui;
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub mod gpu;
pub mod lexer;
pub mod classes;
#[cfg(feature = "jit")]
pub mod jit;
pub mod library;
pub mod model3d;
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

/// The tests of a program: its procedures named `test_...` with no parameters.
pub fn test_names(main: SourceFile, dir: &std::path::Path) -> (Vec<SourceFile>, Result<Vec<String>, Vec<Error>>) {
    let (files, program) = modules::load(main, dir);
    let names = program.map(|p| {
        p.subprograms
            .iter()
            .filter(|s| s.name.starts_with("test_") && !s.is_function && s.params.is_empty())
            .map(|s| s.name.clone())
            .collect()
    });
    (files, names)
}

/// Compiles a program to run one of its tests: the main program first (the
/// setting up), then the procedure `test`.
pub fn compile_test(main: SourceFile, dir: &std::path::Path, test: &str) -> (Vec<SourceFile>, Result<bytecode::Module, Vec<Error>>) {
    let (files, program) = modules::load(main, dir);
    let wants_jit = program.as_ref().is_ok_and(|p| p.imports.iter().any(|i| i.module == "jit"));
    let result = program
        .map(|mut p| {
            let pos = p.subprograms.iter().find(|s| s.name == test).map(|s| s.pos).unwrap_or_default();
            p.body.push(ast::Stmt { kind: ast::StmtKind::Call(test.to_string(), Vec::new()), pos });
            p
        })
        .and_then(|mut p| classes::lower(&mut p).map(|()| p))
        .and_then(|p| checker::check(&p))
        .map(|t| {
            let mut m = compiler::compile(&t);
            m.jit = wants_jit;
            m
        });
    (files, result)
}

/// Compiles a program and the modules it imports (looked for in `dir`, the
/// folder of the program). The files come back too, for rendering errors.
pub fn compile_program(
    main: SourceFile,
    dir: &std::path::Path,
) -> (Vec<SourceFile>, Result<bytecode::Module, Vec<Error>>) {
    let (files, program) = modules::load(main, dir);
    // `import jit`: the program asks for machine code where it is hot
    let wants_jit = program.as_ref().is_ok_and(|p| p.imports.iter().any(|i| i.module == "jit"));
    let result = program
        .and_then(|mut p| classes::lower(&mut p).map(|()| p))
        .and_then(|p| checker::check(&p))
        .map(|t| {
            let mut m = compiler::compile(&t);
            m.jit = wants_jit;
            m
        });
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
/// Runs a program, tracing each executed source line to stderr (`--trace`).
pub fn run_traced(
    module: &bytecode::Module,
    files: &[SourceFile],
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<(), Error> {
    vm::run_traced(module, files, input, output)
}

/// Runs a program under a debugger (see `debug::DebugHook`).
pub fn run_debug<'m>(
    module: &'m bytecode::Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    hook: Box<dyn debug::DebugHook + 'm>,
) -> Result<(), Error> {
    vm::run_debug(module, input, output, hook)
}

pub fn run(module: &bytecode::Module, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
    vm::run(module, input, output)
}
