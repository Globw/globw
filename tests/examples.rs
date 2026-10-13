use std::fs;
use std::path::Path;

/// Output of an example as a user would see it: program output followed by
/// the rendered error, if any.
fn run_example(path: &Path) -> String {
    let src = fs::read_to_string(path).unwrap();
    let input = fs::read_to_string(path.with_extension("input")).unwrap_or_default();
    match globw::compile(&src) {
        Err(errors) => errors.iter().map(|e| e.render(&src)).collect::<Vec<_>>().join("\n"),
        Ok(module) => {
            let mut out = Vec::new();
            let result = globw::run(&module, &mut input.as_bytes(), &mut out);
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
        if path.extension().map_or(true, |e| e != "gw") {
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
