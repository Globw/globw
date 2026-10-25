//! JavaScript entry point for the web playground: compile and run a `.gw`
//! program and return its output (or the errors) as a small JSON string.

use wasm_bindgen::prelude::*;

use crate::{compile_program, run, SourceFile};

/// Runs a Globw program. Returns `{"ok": bool, "out": string}` as JSON:
/// `out` is the program's output, or the compile/run error rendered with a caret.
#[wasm_bindgen]
pub fn run_source(src: &str) -> String {
    let (files, compiled) = compile_program(
        SourceFile { name: "program.gw".to_string(), text: src.to_string() },
        std::path::Path::new("."),
    );
    let module = match compiled {
        Ok(m) => m,
        Err(errors) => {
            let text: String = errors.iter().map(|e| e.render_in(&files)).collect();
            return result_json(false, &text);
        }
    };
    // the playground has no interactive input, so `input()` reads from nothing
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut out: Vec<u8> = Vec::new();
    let outcome = run(&module, &mut input, &mut out);
    let printed = String::from_utf8_lossy(&out).into_owned();
    match outcome {
        Ok(()) => result_json(true, &printed),
        Err(e) => result_json(false, &format!("{}{}", printed, e.render_in(&files))),
    }
}

fn result_json(ok: bool, out: &str) -> String {
    serde_json::json!({ "ok": ok, "out": out }).to_string()
}
