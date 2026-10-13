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
