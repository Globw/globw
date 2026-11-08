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
    // Permissions are denied by default; each --allow-* flag grants one. This is
    // the security boundary for running untrusted .gw files and gwget packages.
    let mut check_only = false;
    let mut profile = false;
    let mut json = false;
    let mut dump_builtins = false;
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
                "Usage: gw [options] program.gw\n\nOptions:\n  --repl           start an interactive session (REPL)\n  --check          type-check only, do not run\n  --json           report errors as JSON (for editors/CI)\n  --builtins       print the builtin catalogue as JSON (for editors)\n  --profile        run with an opcode profile\n  --allow-run      permit shell.run and the proc module\n  --allow-net      permit the web/serve/net modules and ai requests\n  --allow-read     permit reading files and databases\n  --allow-write    permit writing files, charts and databases\n  --allow-all      permit everything (use with care)\n\n  gw add <url>     download one .gw module into libs/\n\nWithout any --allow-* flag the program cannot touch the shell, network or\nfiles. For catalogue libraries use gwget, e.g. gwget install <name>."
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
    Stmt,
    Expr,
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

fn assemble(imports: &[String], decls: &[String], stmts: &[String], tail: Option<&str>) -> String {
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
    let (mut imports, mut decls, mut stmts): (Vec<String>, Vec<String>, Vec<String>) = (vec![], vec![], vec![]);
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
        let line = line.trim_end().to_string();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "variables:" {
            continue;
        }
        let (mut ti, mut td, mut ts) = (imports.clone(), decls.clone(), stmts.clone());
        let tail = match classify(&line) {
            Kind::Import => {
                ti.push(line.clone());
                None
            }
            Kind::Decl => {
                td.push(line.clone());
                None
            }
            Kind::Assign => {
                ts.push(line.clone());
                None
            }
            Kind::Stmt => Some(line.clone()),
            Kind::Expr => Some(format!("output({})", line)),
        };
        match repl_eval(&assemble(&ti, &td, &ts, tail.as_deref())) {
            Ok(out) => {
                print!("{}", out);
                let _ = io::stdout().flush();
                imports = ti;
                decls = td;
                stmts = ts;
            }
            Err(e) => eprintln!("{}", e.trim_end()),
        }
    }
    eprintln!();
    ExitCode::SUCCESS
}
