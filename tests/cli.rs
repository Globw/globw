use std::process::Command;

fn easier(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_easier")).args(args).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

#[test]
fn runs_a_program() {
    let (code, stdout, _) = easier(&["examples/hello.ez"]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Hello, world!\n");
}

#[test]
fn check_mode() {
    let (code, stdout, _) = easier(&["--check", "examples/factorial.ez"]);
    assert_eq!((code, stdout.as_str()), (0, "No errors found.\n"));
    let (code, _, stderr) = easier(&["--check", "examples/error_undeclared.ez"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("variable \"count\" is not declared"), "{}", stderr);
}

#[test]
fn runtime_error_exit_code() {
    let (code, _, stderr) = easier(&["examples/error_index.ez"]);
    assert_eq!(code, 2);
    assert!(stderr.starts_with("Error at line 5"), "{}", stderr);
}

#[test]
fn usage_errors() {
    let (code, _, stderr) = easier(&[]);
    assert_eq!(code, 3);
    assert!(stderr.starts_with("Usage:"), "{}", stderr);
    let (code, _, stderr) = easier(&["missing.ez"]);
    assert_eq!(code, 3);
    assert!(stderr.starts_with("cannot read \"missing.ez\""), "{}", stderr);
}
