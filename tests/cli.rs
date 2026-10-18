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

#[test]
fn unknown_type_is_reported() {
    let dir = std::env::temp_dir().join("globw_unknown_type_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.gw"), "variables:\n    x as integer\n").unwrap();
    let (code, _, stderr) = globw(&["--check", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(stderr.contains("unknown type \"integer\""), "{}", stderr);
}

#[test]
fn classes_run_end_to_end() {
    let dir = std::env::temp_dir().join("globw_classes_cli_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "variables:\n    a as Box\n    b as Box\nclass Box\n    variables:\n        n as int\n    procedure new(n as int)\n        self.n = n\n    end procedure\n    procedure add(k as int)\n        self.n = self.n + k\n    end procedure\nend class\na = new Box(10)\nb = a\na.add(5)\noutput(b.n, \" \", a)\n";
    std::fs::write(dir.join("c.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("c.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "15 Box(n=15)\n"), "{}", stderr);
}

#[test]
fn uncaught_error_still_stops_the_program() {
    let dir = std::env::temp_dir().join("globw_uncaught_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.gw"), "variables:\n    e as string\ntry\n    output(1 div 0)\ncatch e\n    output(\"caught\")\nend try\noutput(10 div 0)\n").unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(code, 2);
    assert_eq!(stdout, "caught\n");
    assert!(stderr.contains("division by zero"), "{}", stderr);
}

#[test]
fn builtin_module_text_regex() {
    let dir = std::env::temp_dir().join("globw_text_module_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.gw"), "import text\noutput(text.matches(\"a1b2\", \"[0-9]\"), \" \", text.replace_all(\"a1b2\", \"[0-9]\", \"#\"))\n").unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "true a#b#\n"), "{}", stderr);
}

#[test]
fn builtin_module_hash_digests() {
    let dir = std::env::temp_dir().join("globw_hash_module_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.gw"), "import hash\noutput(hash.sha256(\"abc\"))\n").unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n");
}

#[test]
fn builtin_module_data_json() {
    let dir = std::env::temp_dir().join("globw_data_module_test");
    std::fs::create_dir_all(&dir).unwrap();
    // a[2] is 1-based, to_json serializes a Globw list
    let src = "import data\noutput(data.number(\"{\\\"a\\\":[5,6]}\", \"a[2]\"), \" \", data.to_json([1, 2]))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "6.0 [1,2]\n"), "{}", stderr);
}

#[test]
fn builtin_module_clock_roundtrip() {
    let dir = std::env::temp_dir().join("globw_clock_module_test");
    std::fs::create_dir_all(&dir).unwrap();
    // parse then format with the same pattern is independent of the local time zone
    let src = "import clock\noutput(clock.format(clock.parse(\"2021-06-15 12:30:00\", \"%Y-%m-%d %H:%M:%S\"), \"%Y-%m-%d %H:%M:%S\"))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "2021-06-15 12:30:00\n"), "{}", stderr);
}

#[test]
fn builtin_modules_batch() {
    let dir = std::env::temp_dir().join("globw_batch_module_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import math\nimport roman\nimport base\nimport str\nimport stats\nimport luhn\n\
               output(math.gcd(24, 36), \" \", math.factorial(5))\n\
               output(roman.to(2024), \" \", base.to(255, 16))\n\
               output(str.pad_left(\"7\", 3, \"0\"), \" \", stats.median([3.0, 1.0, 2.0]))\n\
               output(luhn.valid(\"4539578763621486\"))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "12 120\nMMXXIV ff\n007 2.0\ntrue\n"), "{}", stderr);
}

#[test]
fn builtin_modules_big_batch() {
    let dir = std::env::temp_dir().join("globw_big_batch_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import choose\nimport roman\nimport base32\nimport collatz\n\
               output(choose.combinations(49, 6), \" \", roman.to(1994))\n\
               output(base32.decode(base32.encode(\"hi\")), \" \", collatz.steps(27))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "13983816 MCMXCIV\nhi 111\n"), "{}", stderr);
}

#[test]
fn builtin_module_grid_engine() {
    let dir = std::env::temp_dir().join("globw_grid_module_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    a, c as nungw of float\na = nungw.of([3.0, 4.0])\n\
               output(nungw.sum(a), \" \", nungw.dot(a, a), \" \", nungw.norm(a))\n\
               c = a * 2.0\noutput(nungw.to_list(c), \" \", a[2])\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "7.0 25.0 5.0\n[6.0, 8.0] 4.0\n"), "{}", stderr);
}

#[test]
fn builtin_module_encode_base64() {
    let dir = std::env::temp_dir().join("globw_encode_module_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import encode\noutput(encode.base64(\"hi\"), \" \", encode.from_base64(encode.base64(\"round\")))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "aGk= round\n"), "{}", stderr);
}
