use std::process::Command;

fn globw(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_globw")).args(args).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

#[test]
fn runs_a_program() {
    let (code, stdout, _) = globw(&["examples/hello.gw"]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Hello, world!\n");
}

#[test]
fn check_mode() {
    let (code, stdout, _) = globw(&["--check", "examples/factorial.gw"]);
    assert_eq!((code, stdout.as_str()), (0, "No errors found.\n"));
    let (code, _, stderr) = globw(&["--check", "examples/error_undeclared.gw"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("variable \"count\" is not declared"), "{}", stderr);
}

#[test]
fn runtime_error_exit_code() {
    let (code, _, stderr) = globw(&["examples/error_index.gw"]);
    assert_eq!(code, 2);
    assert!(stderr.starts_with("Error at line 5"), "{}", stderr);
}

#[test]
fn usage_errors() {
    let (code, _, stderr) = globw(&[]);
    assert_eq!(code, 3);
    assert!(stderr.starts_with("Usage:"), "{}", stderr);
    let (code, _, stderr) = globw(&["missing.gw"]);
    assert_eq!(code, 3);
    assert!(stderr.starts_with("cannot read \"missing.gw\""), "{}", stderr);
}

#[test]
fn non_utf8_file_is_explained() {
    let path = std::env::temp_dir().join("globw_cp1252_test.gw");
    std::fs::write(&path, b"output(\"perch\xe8\")\n").unwrap();
    let (code, _, stderr) = globw(&[path.to_str().unwrap()]);
    assert_eq!(code, 3);
    assert!(stderr.contains("is not saved as UTF-8"), "{}", stderr);
}

#[test]
fn modules_and_lazy_imports() {
    let (code, stdout, stderr) = globw(&["examples/modules/main.gw"]);
    assert_eq!(code, 0, "{}", stderr);
    let expected = std::fs::read_to_string("examples/modules/main.expected").unwrap().replace("\r\n", "\n");
    assert_eq!(stdout, expected);
}

#[test]
fn errors_in_modules_name_their_file() {
    let dir = std::env::temp_dir().join("globw_module_errors_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tools.gw"), "function half(n as int) as int\n    return n div 0\nend function\n").unwrap();
    std::fs::write(dir.join("broken.gw"), "function f() as int\n    return 1 +\nend function\n").unwrap();
    std::fs::write(dir.join("ok.gw"), "import tools\noutput(tools.half(4))\n").unwrap();
    std::fs::write(dir.join("bad.gw"), "import broken\n").unwrap();
    std::fs::write(dir.join("missing.gw"), "import nothing_here\n").unwrap();
    let path = |f: &str| dir.join(f).to_str().unwrap().to_string();
    let (code, _, stderr) = globw(&[&path("ok.gw")]);
    assert_eq!(code, 2);
    assert!(stderr.starts_with("Error in tools.gw at line 2, column 14: division by zero"), "{}", stderr);
    let (code, _, stderr) = globw(&["--check", &path("bad.gw")]);
    assert_eq!(code, 1);
    assert!(stderr.starts_with("Error in broken.gw at line 2"), "{}", stderr);
    let (code, _, stderr) = globw(&["--check", &path("missing.gw")]);
    assert_eq!(code, 1);
    assert!(stderr.contains("module \"nothing_here\" not found"), "{}", stderr);
}

#[test]
fn module_locals_may_share_names_with_other_files_functions() {
    let dir = std::env::temp_dir().join("globw_module_names_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("calc.gw"), "function need(goal as float) as float\n    return goal * 2\nend function\n").unwrap();
    std::fs::write(dir.join("main.gw"), "import calc\nprocedure goal()\n    output(calc.need(2))\nend procedure\ngoal()\n").unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("main.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "4.0\n"), "{}", stderr);
}
