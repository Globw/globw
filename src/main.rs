use std::io::{self, BufWriter, Write};
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
    let (check_only, profile, file) = match args.as_slice() {
        [f] if !f.starts_with("--") => (false, false, f),
        [flag, f] if flag == "--check" => (true, false, f),
        [flag, f] if flag == "--profile" => (false, true, f),
        _ => {
            eprintln!(
                "Usage: globw program.gw\n       globw --check program.gw\n       globw --profile program.gw\n       globw add <url>   (download a .gw module into libs/)"
            );
            return ExitCode::from(3);
        }
    };
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
            let text: Vec<String> = errors.iter().map(|e| e.render_in(&files)).collect();
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
