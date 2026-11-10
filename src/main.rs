use std::io::{self, BufRead, BufWriter, Write};
use std::process::ExitCode;

/// `globw add <url>`: downloads a .gw module into a local `libs/` folder, where
/// `import` can then find it.
fn globw_add(url: &str) -> ExitCode {
    let base = url.rsplit('/').next().unwrap_or("").split(['?', '#']).next().unwrap_or("");
    let file = if base.ends_with(".gw") { base.to_string() } else { format!("{}.gw", base) };
    if file == ".gw" || file.is_empty() {
        eprintln!("globw add: could not work out a module name from \"{}\"", url);
        return ExitCode::from(3);
    }
    let body = match ureq::get(url).timeout(std::time::Duration::from_secs(60)).call() {
        Ok(r) => match r.into_string() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("globw add: cannot read the reply from {}: {}", url, e);
                return ExitCode::from(1);
            }
        },
        Err(e) => {
            eprintln!("globw add: cannot download {}: {}", url, e);
            return ExitCode::from(1);
        }
    };
    let dir = std::path::Path::new("libs");
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("globw add: cannot create the libs folder: {}", e);
        return ExitCode::from(1);
    }
    let path = dir.join(&file);
    if let Err(e) = std::fs::write(&path, body) {
        eprintln!("globw add: cannot write {}: {}", path.display(), e);
        return ExitCode::from(1);
    }
    println!("added {} - use it with: import {}", path.display(), file.trim_end_matches(".gw"));
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [cmd, url] = args.as_slice() {
        if cmd == "add" {
            return globw_add(url);
        }
    }
    if args.first().map(String::as_str) == Some("fmt") {
        return globw_fmt(&args[1..]);
    }
    // Permissions are denied by default; each --allow-* flag grants one. This is
    // the security boundary for running untrusted .gw files and gwget packages.
    let mut check_only = false;
    let mut profile = false;
    let mut json = false;
    let mut dump_builtins = false;
    let mut dump_symbols = false;
    let mut repl_mode = false;
    let mut caps = globw::library::Caps::none();
    let mut file: Option<&String> = None;
    let mut bad = false;
    for a in &args {
        let (key, val) = match a.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (a.as_str(), None),
        };
        match key {
            "--check" => check_only = true,
            "--profile" => profile = true,
            "--json" => json = true,
            "--builtins" => dump_builtins = true,
            "--symbols" => dump_symbols = true,
            "--repl" => repl_mode = true,
            "--allow-run" => caps.allow_run(),
            "--allow-net" => caps.allow_net(val),
            "--allow-read" => caps.allow_read(val),
            "--allow-write" => caps.allow_write(val),
            "--allow-all" => caps.allow_all(),
            s if s.starts_with("--") => {
                eprintln!("unknown option: {}", a);
                bad = true;
            }
            _ => {
                if file.is_none() {
                    file = Some(a);
                }
            }
        }
    }
    if dump_builtins {
        // the builtin catalogue, for editor completion and hover; no file needed
        println!("{}", globw::builtins::catalogue_json());
        return ExitCode::SUCCESS;
    }
    if repl_mode {
        return repl(caps);
    }
    let file = match (bad, file) {
        (false, Some(f)) => f,
        _ => {
            eprintln!(
                "Usage: gw [options] program.gw\n\nOptions:\n  --repl           start an interactive session (REPL)\n  --check          type-check only, do not run\n  --json           report errors as JSON (for editors/CI)\n  --builtins       print the builtin catalogue as JSON (for editors)\n  --symbols        print this file's definitions as JSON (for editors)\n  --profile        run with an opcode profile\n  --allow-run      permit shell.run and the proc module\n  --allow-net      permit the web/serve/net modules and ai requests\n  --allow-read     permit reading files and databases\n  --allow-write    permit writing files, charts and databases\n  --allow-all      permit everything (use with care)\n\n  gw fmt [files]   re-indent .gw files (or standard input to standard output)\n  gw add <url>     download one .gw module into libs/\n\nWithout any --allow-* flag the program cannot touch the shell, network or\nfiles. For catalogue libraries use gwget, e.g. gwget install <name>."
            );
            return ExitCode::from(3);
        }
    };
    globw::library::set_caps(caps);
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            eprintln!(
                "cannot read \"{}\": it is not saved as UTF-8\nHint: in VS Code click the encoding in the status bar \
                 (bottom right), choose \"Save with Encoding\" and then UTF-8",
                file
            );
            return ExitCode::from(3);
        }
        Err(e) => {
            eprintln!("cannot read \"{}\": {}", file, e);
            return ExitCode::from(3);
        }
    };
    if dump_symbols {
        // definitions in this file (for editor outline and go-to-definition)
        println!("{}", symbols_json(&source));
        return ExitCode::SUCCESS;
    }
    // relative paths in the program (for example font files and modules) start from its folder
    if let Some(dir) = std::path::Path::new(file).parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = std::env::set_current_dir(dir);
    }
    let name = std::path::Path::new(file).file_name().map_or(file.clone(), |n| n.to_string_lossy().into_owned());
    let (files, compiled) = globw::compile_program(globw::SourceFile { name, text: source }, std::path::Path::new("."));
    let module = match compiled {
        Ok(m) => m,
        Err(errors) => {
            if json {
                // machine-readable diagnostics, for editors / CI
                let arr: Vec<serde_json::Value> = errors
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "file": files.get(e.pos.file as usize).map(|f| f.name.clone()),
                            "line": e.pos.line,
                            "col": e.pos.col,
                            "len": e.pos.len,
                            "message": e.message,
                            "hint": e.hint,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string()));
            } else {
                let text: Vec<String> = errors.iter().map(|e| e.render_in(&files)).collect();
                eprint!("{}", text.join("\n"));
            }
            return ExitCode::from(1);
        }
    };
    if check_only {
        if json {
            println!("[]");
        } else {
            println!("No errors found.");
        }
        return ExitCode::SUCCESS;
    }
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    if profile {
        let result = globw::run_profiled(&module, &mut input, &mut output);
        let _ = output.flush();
        return match result {
            Ok(counts) => {
                let total: u64 = counts.iter().map(|(_, n)| n).sum();
                eprintln!("\n--- opcode profile: {} instructions ---", total);
                for (name, n) in counts.iter().take(20) {
                    eprintln!("{:>12}  {:>5.1}%  {}", n, 100.0 * *n as f64 / total.max(1) as f64, name);
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprint!("{}", e.render_in(&files));
                ExitCode::from(2)
            }
        };
    }
    let result = globw::run(&module, &mut input, &mut output);
    let _ = output.flush();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprint!("{}", e.render_in(&files));
            ExitCode::from(2)
        }
    }
}

// ---- the interactive REPL (`gw --repl`) ----
// It has no persistent VM, so it keeps the session's imports, declarations and
// assignments and re-runs them before each new line, printing only that line's
// output. Correct for pure computation (the REPL's purpose); a declaration or
// assignment with side effects would re-run. Multi-line blocks are not supported.

enum Kind {
    Import,
    Decl,
    Assign,
    Def,
    Stmt,
    Expr,
}

/// The leading identifier of a line (its first keyword).
fn first_word(line: &str) -> String {
    line.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect()
}

/// Whether the line opens a multi-line block that a matching `end` closes.
fn is_block_opener(line: &str) -> bool {
    matches!(
        first_word(line).as_str(),
        "for" | "while" | "if" | "function" | "procedure" | "class" | "try"
    )
}

/// The name a function/procedure/class block defines (its second word).
fn def_name(block: &str) -> String {
    block
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// +1 for a block opener, -1 for `end`, 0 otherwise -- for tracking nesting.
fn block_delta(line: &str) -> i32 {
    match first_word(line).as_str() {
        "for" | "while" | "if" | "function" | "procedure" | "class" | "try" => 1,
        "end" => -1,
        _ => 0,
    }
}

/// The first `=` at bracket depth 0 that is a plain assignment (not ==, !=, <=, >=
/// and not inside a string or brackets).
fn find_assign_eq(t: &str) -> Option<usize> {
    let b = t.as_bytes();
    let (mut depth, mut in_str) = (0i32, false);
    for i in 0..b.len() {
        let c = b[i];
        if in_str {
            if c == b'"' {
                in_str = false;
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 => {
                let prev = if i > 0 { b[i - 1] } else { 0 };
                let next = if i + 1 < b.len() { b[i + 1] } else { 0 };
                if next != b'=' && !matches!(prev, b'=' | b'!' | b'<' | b'>') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn classify(line: &str) -> Kind {
    let t = line.trim_start();
    if t.starts_with("import ") || t.starts_with("lazy ") || t.starts_with("from ") {
        return Kind::Import;
    }
    const KW: &[&str] = &[
        "output", "input", "append", "remove", "for", "while", "if", "else", "end", "return",
        "function", "procedure", "class", "try", "catch", "always", "break", "continue", "do",
    ];
    let first: String = t.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    if KW.contains(&first.as_str()) {
        return Kind::Stmt;
    }
    if let Some(idx) = t.find(" as ") {
        let before = &t[..idx];
        if !before.is_empty() && before.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ',' || c == ' ') {
            return Kind::Decl;
        }
    }
    if let Some(eq) = find_assign_eq(t) {
        if t[..eq].trim().chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
            return Kind::Assign;
        }
    }
    Kind::Expr
}

fn assemble(imports: &[String], decls: &[String], defs: &[String], stmts: &[String], tail: Option<&str>) -> String {
    let mut s = String::new();
    for i in imports {
        s.push_str(i);
        s.push('\n');
    }
    if !decls.is_empty() {
        s.push_str("variables:\n");
        for d in decls {
            s.push_str("    ");
            s.push_str(d.trim_start());
            s.push('\n');
        }
    }
    for d in defs {
        s.push_str(d);
        s.push('\n');
    }
    for st in stmts {
        s.push_str(st);
        s.push('\n');
    }
    if let Some(t) = tail {
        s.push_str(t);
        s.push('\n');
    }
    s
}

/// Compiles and runs one assembled REPL program, returning its captured output.
fn repl_eval(src: &str) -> Result<String, String> {
    let (files, compiled) = globw::compile_program(
        globw::SourceFile { name: "repl".to_string(), text: src.to_string() },
        std::path::Path::new("."),
    );
    let module = compiled.map_err(|errors| errors.iter().map(|e| e.render_in(&files)).collect::<Vec<_>>().join("\n"))?;
    let mut out: Vec<u8> = Vec::new();
    let mut input = io::Cursor::new(Vec::<u8>::new());
    globw::run(&module, &mut input, &mut out).map_err(|e| e.render_in(&files))?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn repl(caps: globw::library::Caps) -> ExitCode {
    globw::library::set_caps(caps);
    eprintln!("Globw REPL. Type an expression to see its value, a declaration like `x as int`,");
    eprintln!("an assignment like `x = 5`, or `import nungw`. Ctrl-D (Ctrl-Z on Windows) to quit.");
    let (mut imports, mut decls, mut defs, mut stmts): (Vec<String>, Vec<String>, Vec<String>, Vec<String>) =
        (vec![], vec![], vec![], vec![]);
    let stdin = io::stdin();
    let mut lock = stdin.lock();
    loop {
        eprint!(">>> ");
        let _ = io::stderr().flush();
        let mut line = String::new();
        match lock.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let mut block = line.trim_end().to_string();
        if block.trim().is_empty() || block.trim() == "variables:" {
            continue;
        }
        // read the rest of a multi-line block until its matching `end`
        if is_block_opener(&block) {
            let mut depth = 1;
            while depth > 0 {
                eprint!("... ");
                let _ = io::stderr().flush();
                let mut cont = String::new();
                match lock.read_line(&mut cont) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let cont = cont.trim_end();
                depth += block_delta(cont);
                block.push('\n');
                block.push_str(cont);
            }
        }
        let kind = match first_word(&block).as_str() {
            "function" | "procedure" | "class" => Kind::Def,
            _ if is_block_opener(&block) => Kind::Stmt,
            _ => classify(&block),
        };
        let (mut ti, mut td, mut tf, mut ts) = (imports.clone(), decls.clone(), defs.clone(), stmts.clone());
        let tail = match kind {
            Kind::Import => {
                ti.push(block.clone());
                None
            }
            Kind::Decl => {
                // re-declaring a name replaces the earlier declaration
                let lhs = block.split(" as ").next().unwrap_or("").trim().to_string();
                td.retain(|d| d.split(" as ").next().unwrap_or("").trim() != lhs);
                td.push(block.clone());
                None
            }
            Kind::Def => {
                // re-defining a function/procedure/class replaces the earlier one
                let name = def_name(&block);
                tf.retain(|d| def_name(d) != name);
                tf.push(block.clone());
                None
            }
            Kind::Assign => {
                ts.push(block.clone());
                None
            }
            Kind::Stmt => Some(block.clone()),
            Kind::Expr => Some(format!("output({})", block)),
        };
        match repl_eval(&assemble(&ti, &td, &tf, &ts, tail.as_deref())) {
            Ok(out) => {
                print!("{}", out);
                let _ = io::stdout().flush();
                imports = ti;
                decls = td;
                defs = tf;
                stmts = ts;
            }
            Err(e) => eprintln!("{}", e.trim_end()),
        }
    }
    eprintln!();
    ExitCode::SUCCESS
}

// ---- the formatter (`gw fmt`) ----
// Line-based: it re-indents each line to its block depth and normalises blank
// lines and trailing spaces, without touching the content of a line. Globw is
// not whitespace-sensitive (blocks close with `end`), so this is safe -- it
// never reformats expressions or strings, and comments are preserved.

/// Whether a line declares variables (`<names> as <type>`), used to indent the
/// body of a `variables:` block.
fn is_decl_line(t: &str) -> bool {
    match t.find(" as ") {
        Some(idx) => {
            let before = &t[..idx];
            !before.is_empty() && before.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ',' || c == ' ')
        }
        None => false,
    }
}

/// Re-indents `src` to canonical form.
fn format_source(src: &str) -> String {
    let mut out = String::new();
    let mut stack: Vec<&str> = Vec::new(); // the open blocks, by kind
    let mut in_vars = false; // inside a `variables:` block (its decls get one more level)
    let mut blanks = 0;
    for raw in src.lines() {
        let t = raw.trim();
        if t.is_empty() {
            blanks += 1;
            in_vars = false; // a blank line ends a `variables:` block
            if blanks <= 2 {
                // keep up to one blank between items and up to two between sections
                out.push('\n');
            }
            continue;
        }
        blanks = 0;
        let comment = t.starts_with('\'');
        let first = first_word(t);
        let vars_header = t == "variables:" || t == "global variables:";
        // a `variables:` block ends at the first line that is not a declaration
        if in_vars && !comment && !is_decl_line(t) && !vars_header {
            in_vars = false;
        }
        // `while(...)` after `repeat` is the loop's closer, not an opener
        let closes_repeat = first == "while" && stack.last() == Some(&"repeat");
        if first == "end" || closes_repeat {
            stack.pop();
        }
        let dedent_kw = matches!(first.as_str(), "else" | "catch" | "always");
        let level = (stack.len() as i32 + i32::from(in_vars) - i32::from(dedent_kw)).max(0);
        for _ in 0..level {
            out.push_str("    ");
        }
        out.push_str(t);
        out.push('\n');
        if vars_header {
            in_vars = true;
        } else if !closes_repeat {
            match first.as_str() {
                "for" | "while" => stack.push("loop"),
                "if" => stack.push("if"),
                "function" => stack.push("function"),
                "procedure" => stack.push("procedure"),
                "class" => stack.push("class"),
                "try" => stack.push("try"),
                "repeat" => stack.push("repeat"),
                _ => {}
            }
        }
        // inline multi-line lambdas: `function(` away from the start of the line
        // opens a block too. Skip declarations (`x as function(...) as T` is a
        // type) and one-liners (`function(...) gives expr` closes on its own line).
        if !comment && !is_decl_line(t) && !t.contains(" gives ") {
            let extra = t.matches("function(").count().saturating_sub(usize::from(t.starts_with("function(")));
            for _ in 0..extra {
                stack.push("function");
            }
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn globw_fmt(args: &[String]) -> ExitCode {
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if files.is_empty() {
        // no file: format standard input to standard output (for editors)
        match io::read_to_string(io::stdin()) {
            Ok(src) => {
                print!("{}", format_source(&src));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("fmt: cannot read standard input: {}", e);
                ExitCode::from(1)
            }
        }
    } else {
        let mut code = ExitCode::SUCCESS;
        for f in files {
            match std::fs::read_to_string(f) {
                Ok(src) => {
                    let formatted = format_source(&src);
                    if formatted == src {
                        println!("{} is already formatted", f);
                    } else if let Err(e) = std::fs::write(f, &formatted) {
                        eprintln!("fmt: cannot write {}: {}", f, e);
                        code = ExitCode::from(1);
                    } else {
                        println!("formatted {}", f);
                    }
                }
                Err(e) => {
                    eprintln!("fmt: cannot read {}: {}", f, e);
                    code = ExitCode::from(1);
                }
            }
        }
        code
    }
}

// ---- `gw --symbols`: definitions in a file, for the editor ----

fn json_quote(s: &str) -> String {
    // identifiers never contain quotes; replace defensively to keep the JSON valid
    format!("\"{}\"", s.replace('"', "'"))
}

fn push_sym(out: &mut Vec<String>, name: &str, kind: &str, pos: &globw::errors::Pos, container: &str) {
    if pos.file != 0 {
        return; // only definitions from this file
    }
    out.push(format!(
        "{{\"name\":{},\"kind\":\"{}\",\"line\":{},\"col\":{},\"len\":{},\"container\":{}}}",
        json_quote(name),
        kind,
        pos.line,
        pos.col,
        pos.len,
        json_quote(container)
    ));
}

/// The definitions in `src` (functions, procedures, classes, methods, fields and
/// global variables) as a JSON array, for the editor's outline and go-to-definition.
fn symbols_json(src: &str) -> String {
    let program = match globw::lexer::tokenize(src).and_then(globw::parser::parse) {
        Ok(p) => p,
        Err(_) => return "[]".to_string(), // a file mid-edit: no symbols rather than an error
    };
    let mut out: Vec<String> = Vec::new();
    for d in &program.globals {
        push_sym(&mut out, &d.name, "variable", &d.pos, "");
    }
    for s in &program.subprograms {
        push_sym(&mut out, &s.name, if s.is_function { "function" } else { "procedure" }, &s.pos, "");
    }
    for c in &program.classes {
        push_sym(&mut out, &c.name, "class", &c.pos, "");
        for f in &c.fields {
            push_sym(&mut out, &f.name, "field", &f.pos, &c.name);
        }
        for m in &c.methods {
            push_sym(&mut out, &m.name, "method", &m.pos, &c.name);
        }
    }
    format!("[{}]", out.join(","))
}
