use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (check_only, file) = match args.as_slice() {
        [f] if f != "--check" => (false, f),
        [flag, f] if flag == "--check" => (true, f),
        _ => {
            eprintln!("Usage: globw program.gw\n       globw --check program.gw");
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
    let module = match globw::compile(&source) {
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
    let result = globw::run(&module, &mut input, &mut output);
    let _ = output.flush();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprint!("{}", e.render(&source));
            ExitCode::from(2)
        }
    }
}
