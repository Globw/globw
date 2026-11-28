use std::process::Command;

fn globw(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gw")).args(args).output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

fn globw_stdin(args: &[&str], input: &str) -> (i32, String, String) {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_gw"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

#[test]
fn repl_evaluates_and_keeps_state() {
    // declarations and assignments persist across lines; a bare expression prints
    // its value. Prompts and errors go to stderr, results to stdout.
    let (code, stdout, stderr) = globw_stdin(&["--repl"], "x as int\nx = 5\nx + 10\nx * 2\n\"hi\"\n");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "15\n10\nhi\n", "{}", stderr);
    // a bad line is reported but does not break the session or the state
    let (code, stdout, stderr) = globw_stdin(&["--repl"], "x as int\nx = 7\nnope + 1\nx + 1\n");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "8\n", "{}", stderr);
    assert!(stderr.contains("not declared"), "{}", stderr);
}

#[test]
fn repl_blocks_and_definitions() {
    // a function definition spans several lines, persists, and is callable; a
    // redefinition replaces it; a for loop runs (its counter is declared first).
    let input = "function f() as int\n    return 1\nend function\nf()\n\
        function f() as int\n    return 9\nend function\nf()\n\
        i as int\nfor(i from 1 to 3)\n    output(i)\nend loop\n";
    let (code, stdout, stderr) = globw_stdin(&["--repl"], input);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "1\n9\n1\n2\n3\n", "{}", stderr);
}

#[test]
fn trace_prints_executed_lines() {
    // --trace prints each executed source line to stderr; program output stays on stdout
    let path = std::env::temp_dir().join("gw_trace_test.gw");
    std::fs::write(&path, "variables:\n    s as int\ns = 0\noutput(s)\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gw")).arg("--trace").arg(&path).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("s = 0") && stderr.contains("output(s)"), "{}", stderr);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn runtime_error_shows_a_backtrace() {
    // an uncaught error inside nested calls prints the call stack
    let path = std::env::temp_dir().join("gw_backtrace_test.gw");
    std::fs::write(
        &path,
        "function inner(x as int) as int\n    return 10 div x\nend function\n\
         function outer(n as int) as int\n    return inner(n)\nend function\noutput(outer(0))\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gw")).arg(&path).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("division by zero"), "{}", stderr);
    assert!(stderr.contains("Call stack"), "{}", stderr);
    // the backtrace shows each frame's parameters and their values
    assert!(stderr.contains("in inner(x=0)") && stderr.contains("called from outer(n=0)"), "{}", stderr);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn symbols_lists_definitions() {
    // `gw --symbols` prints a file's definitions (for the editor outline / go-to-def)
    let (code, stdout, stderr) = globw(&["--symbols", "examples/classes.gw"]);
    assert_eq!(code, 0, "{}", stderr);
    assert!(stdout.starts_with('['), "a JSON array: {}", stdout);
    assert!(stdout.contains("\"name\":\"Player\",\"kind\":\"class\""), "lists the class: {}", stdout);
    assert!(stdout.contains("\"kind\":\"method\""), "lists methods: {}", stdout);
}

#[test]
fn fmt_reindents_and_is_idempotent() {
    // from standard input, fmt restores block indentation (four spaces per level)
    let flat = "variables:\nx as int\nx = 5\nif(x > 0)\noutput(x)\nend if\n";
    let want = "variables:\n    x as int\nx = 5\nif(x > 0)\n    output(x)\nend if\n";
    let (code, stdout, stderr) = globw_stdin(&["fmt"], flat);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, want, "{}", stderr);
    // formatting the result again changes nothing
    let (_, again, _) = globw_stdin(&["fmt"], &stdout);
    assert_eq!(again, want);
}

#[test]
fn fmt_indents_match_arms() {
    // a `match` inside an `if`: arms one level in, their bodies two, `else` arm too
    let flat = "if(true)
match n
when < 0, > 9
output(1)
when 1 to 5
if(n == 2)
output(2)
end if
else
output(3)
end match
end if
";
    let want = "if(true)
    match n
        when < 0, > 9
            output(1)
        when 1 to 5
            if(n == 2)
                output(2)
            end if
        else
            output(3)
    end match
end if
";
    let (code, stdout, stderr) = globw_stdin(&["fmt"], flat);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, want);
    let (_, again, _) = globw_stdin(&["fmt"], &stdout);
    assert_eq!(again, want);
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
fn symbolic_differentiation() {
    let dir = std::env::temp_dir().join("globw_symbol_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("y.gw");
    std::fs::write(
        &prog,
        "import symbol\nvariables:\n    x, f, df as int\nsymbol.reset()\nx = symbol.var(\"x\")\n\
         f = symbol.pow(x, symbol.const(3.0))\ndf = symbol.simplify(symbol.diff(f, x))\n\
         output(symbol.eval(df, x, 2.0))\noutput(symbol.text(symbol.simplify(symbol.diff(symbol.sin(x), x))))\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "12.0\ncos(x)\n");
}

#[test]
fn audio_tone_fft_and_wav_roundtrip() {
    let dir = std::env::temp_dir().join("globw_synth_test");
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("t.wav").to_str().unwrap().replace('\\', "/");
    let prog = dir.join("s.gw");
    std::fs::write(
        &prog,
        format!(
            "import synth\nvariables:\n    a, b as int\nsynth.rate(8000)\na = synth.tone(440.0, 0.05, \"sine\")\n\
             output(synth.length(a))\noutput(synth.dominant(a))\nsynth.save_wav(a, \"{wav}\")\n\
             b = synth.load_wav(\"{wav}\")\noutput(synth.dominant(b))\n"
        ),
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "400\n440.0\n440.0\n");
}

#[test]
fn physics_gravity_and_collision() {
    let dir = std::env::temp_dir().join("globw_sim_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("s.gw");
    std::fs::write(
        &prog,
        "import gwsim\nvariables:\n    a, b as int\ngwsim.reset()\ngwsim.gravity(0.0, 500.0)\n\
         a = gwsim.body(100.0, 100.0, 10.0, 1.0)\ngwsim.step(0.1)\noutput(gwsim.vy(a))\noutput(gwsim.y(a))\n\
         gwsim.reset()\ngwsim.bounds(0.0, 0.0, 400.0, 400.0, 0.5)\na = gwsim.body(100.0, 100.0, 20.0, 1.0)\n\
         b = gwsim.body(130.0, 100.0, 20.0, 1.0)\noutput(gwsim.collide(a, b))\ngwsim.step(0.05)\ngwsim.step(0.05)\noutput(gwsim.collide(a, b))\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "50.0\n105.0\ntrue\nfalse\n");
}

#[test]
fn dataframe_csv_aggregates_and_groupby() {
    let dir = std::env::temp_dir().join("globw_frame_test");
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("d.csv");
    std::fs::write(&csv, "dept,salary\n1,50\n1,60\n2,80\n2,90\n1,55\n").unwrap();
    let prog = dir.join("f.gw");
    let src = format!(
        "import gwframe\nvariables:\n    f, g as int\nf = gwframe.from_csv(\"{}\")\n\
         output(gwframe.sum(f, \"salary\"))\noutput(gwframe.rows(gwframe.filter(f, \"salary\", \">\", 60.0)))\n\
         g = gwframe.group_sum(f, \"dept\", \"salary\")\noutput(gwframe.get(g, 1, \"salary\"))\noutput(gwframe.get(g, 2, \"salary\"))\n",
        csv.to_str().unwrap().replace('\\', "/")
    );
    std::fs::write(&prog, src).unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "335.0\n2\n165.0\n170.0\n");
}

#[test]
fn autodiff_gradients_and_training() {
    let dir = std::env::temp_dir().join("globw_grad_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("g.gw");
    std::fs::write(
        &prog,
        "import grad\nvariables:\n    x, y, xy, z, xh, c5, d, loss, i as int\n    curr as float\n\
         grad.reset()\nx = grad.var(2.0)\ny = grad.var(3.0)\nxy = grad.mul(x, y)\nz = grad.add(xy, x)\n\
         grad.backward(z)\noutput(grad.grad(x))\noutput(grad.grad(y))\n\
         curr = 0.0\nfor(i from 1 to 200)\n    grad.reset()\n    xh = grad.var(curr)\n    c5 = grad.const(5.0)\n\
         \x20   d = grad.sub(xh, c5)\n    loss = grad.pow(d, 2.0)\n    grad.backward(loss)\n    curr = curr - 0.1 * grad.grad(xh)\nend loop\n\
         output(curr > 4.99 and curr < 5.01)\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "4.0\n2.0\ntrue\n");
}

#[test]
fn unit_conversion_modules_work() {
    let dir = std::env::temp_dir().join("globw_units_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("u.gw");
    std::fs::write(
        &prog,
        "import furlong\nimport carat\nimport phi\noutput(furlong.si(1.0))\noutput(carat.inv(0.4))\noutput(phi.value())\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "201.168\n2000.0\n1.618033988749895\n");
}

#[test]
fn task_channels_block_and_deliver() {
    let dir = std::env::temp_dir().join("globw_chan_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("c.gw");
    std::fs::write(
        &prog,
        "import task\nglobal variables:\n    ch as int\nvariables:\n    p as int\n    m as string\n    prod as function()\nch = task.channel()\nprod = function()\n    task.send(ch, \"hi\")\n    task.send(ch, \"bye\")\nend procedure\np = task.spawn(prod)\nm = task.recv(ch)\noutput(m)\nm = task.recv(ch)\noutput(m)\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "hi\nbye\n");
}

#[test]
fn task_await_waits_for_completion() {
    let dir = std::env::temp_dir().join("globw_await_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("a.gw");
    std::fs::write(
        &prog,
        "import task\nvariables:\n    a as int\n    w as function()\nw = function()\n    output(\"worker\")\nend procedure\na = task.spawn(w)\ntask.await(a)\noutput(\"after\")\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "worker\nafter\n");
}

#[test]
fn cooperative_tasks_spawn_and_yield() {
    let dir = std::env::temp_dir().join("globw_task_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("t.gw");
    std::fs::write(
        &prog,
        "import task\nvariables:\n    t as int\n    w as function()\nw = function()\n    output(\"worker 1\")\n    task.yield()\n    output(\"worker 2\")\nend procedure\nt = task.spawn(w)\noutput(\"main a\")\ntask.yield()\noutput(\"main b\")\ntask.yield()\noutput(\"main c\")\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "main a\nworker 1\nmain b\nworker 2\nmain c\n");
}

#[test]
fn list_comprehension() {
    let dir = std::env::temp_dir().join("globw_comp_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("c.gw");
    std::fs::write(
        &prog,
        "variables:\n    nums, out as list of int\n    x as int\nnums = [1, 2, 3, 4, 5]\nout = [x * x for x in nums if x mod 2 == 1]\noutput(out)\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "[1, 9, 25]\n");
}

#[test]
fn match_statement() {
    let dir = std::env::temp_dir().join("globw_match_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("m.gw");
    std::fs::write(
        &prog,
        "variables:\n    x, i as int\nfor(i from 1 to 4)\n    x = i\n    match x\n        when 1\n            output(\"one\")\n        when 2, 3\n            output(\"two-ish\")\n        else\n            output(\"other\")\n    end match\nend loop\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "one\ntwo-ish\ntwo-ish\nother\n");
}

#[test]
fn const_declarations() {
    let dir = std::env::temp_dir().join("globw_const_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("c.gw");
    std::fs::write(
        &prog,
        "variables:\n    r as float\nconst PI = 3.14\nconst MAX = 100\nr = 2.0\noutput(PI * r)\noutput(MAX)\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&[prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "6.28\n100\n");
    // assigning to a constant is rejected
    let bad = dir.join("b.gw");
    std::fs::write(&bad, "variables:\n    x as int\nconst K = 1\nx = 0\nK = 2\n").unwrap();
    let (code, _, stderr) = globw(&[bad.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(stderr.contains("\"K\" is a constant"), "{}", stderr);
}

#[test]
fn constant_expressions_are_folded() {
    // (5+2)*(5-4) is computed at compile time, so the loop body does no arithmetic
    let dir = std::env::temp_dir().join("globw_fold_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("fold.gw");
    std::fs::write(
        &prog,
        "variables:\n    x, i as int\nx = 0\nfor(i from 1 to 10)\n    x = (5 + 2) * (5 - 4)\nend loop\noutput(x)\n",
    )
    .unwrap();
    let (code, stdout, stderr) = globw(&["--profile", prog.to_str().unwrap()]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "7\n");
    assert!(
        !stderr.contains("MulI") && !stderr.contains("AddIImm") && !stderr.contains("SubIImm"),
        "the constant expression was not folded:\n{}",
        stderr
    );
}

#[test]
fn ai_module_errors_cleanly_when_unreachable() {
    // no network dependency: point at a closed local port and check the clean error
    let dir = std::env::temp_dir().join("globw_ai_test");
    std::fs::create_dir_all(&dir).unwrap();
    let prog = dir.join("a.gw");
    std::fs::write(&prog, "import ai\nai.endpoint(\"http://127.0.0.1:1/v1\")\noutput(ai.ask(\"hi\"))\n").unwrap();
    let (code, _, stderr) = globw(&["--allow-net", prog.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(stderr.contains("ai: cannot reach"), "{}", stderr);
}

#[test]
fn net_url_parsing_and_unicode() {
    let (code, stdout, stderr) = globw(&["examples/net_url.gw"]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "api.github.com\n/repos/x\nnew\nSTRASSE\nwbolg\n");
}

#[test]
fn globw_add_reports_a_clean_error_when_unreachable() {
    let (code, _, stderr) = globw(&["add", "http://127.0.0.1:1/mod.gw"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("globw add: cannot download"), "{}", stderr);
}

#[test]
fn imports_resolve_from_a_libs_folder() {
    let (code, stdout, stderr) = globw(&["tests/fixtures/pkgapp.gw"]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "hello from libs\n");
}

#[test]
fn imports_resolve_from_globw_path() {
    let dir = std::env::temp_dir().join("globw_path_test");
    let lib = dir.join("pkgs");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(dir.join("prog.gw"), "import tool\ntool.run()\n").unwrap();
    std::fs::write(lib.join("tool.gw"), "procedure run()\n    output(\"from GLOBW_PATH\")\nend procedure\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_gw"))
        .arg(dir.join("prog.gw"))
        .env("GLOBW_PATH", &lib)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"), "from GLOBW_PATH\n");
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
fn builtin_modules_counter_and_data() {
    let dir = std::env::temp_dir().join("globw_counter_data_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import counter\nimport data\n\
               output(counter.chars(\"banana\"))\n\
               output(data.type(\"{\\\"n\\\":3}\", \"n\"))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "{'b': 1, 'a': 3, 'n': 2}\nnumber\n"), "{}", stderr);
}

#[test]
fn builtin_module_text_regex_extras() {
    let dir = std::env::temp_dir().join("globw_text_extras_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import text\noutput(text.count(\"a1b2c3\", \"[0-9]\"), \" \", text.group(\"2026-10\", \"([0-9]+)-([0-9]+)\", 2))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "3 10\n"), "{}", stderr);
}

#[test]
fn builtin_module_proc_background() {
    let dir = std::env::temp_dir().join("globw_proc_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import proc\nvariables:\n    h as int\nh = proc.start(\"echo globw\")\noutput(trim(proc.wait(h)))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-run", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "globw\n"), "{}", stderr);
}

#[test]
fn builtin_module_db_sqlite() {
    let dir = std::env::temp_dir().join("globw_db_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import db\nvariables:\n    c, n as int\n\
               c = db.open(\":memory:\")\n\
               n = db.run(c, \"create table t (x int)\")\n\
               n = db.run(c, \"insert into t values (1), (2), (3)\")\n\
               output(db.value(c, \"select sum(x) from t\"), \" \", db.query(c, \"select x from t where x > 1 order by x\"))\n\
               db.close(c)\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-read", "--allow-write", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "6 [{\"x\":2},{\"x\":3}]\n"), "{}", stderr);
}

#[test]
fn builtin_modules_batch_g() {
    let dir = std::env::temp_dir().join("globw_batch_g_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import lucas\nimport soundex\nimport contrast\nimport primorial\n\
               output(lucas.number(10), \" \", soundex.of(\"Robert\"), \" \", round_to(contrast.ratio(\"#000000\", \"#ffffff\"), 1), \" \", primorial.of(5))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "123 R163 21.0 2310\n"), "{}", stderr);
}

#[test]
fn builtin_modules_table_iter_clock() {
    let dir = std::env::temp_dir().join("globw_table_iter_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import table\nimport iter\nimport clock\n\
               output(table.mean(\"x\\n2\\n4\\n6\", \"x\"), \" \", iter.combinations(\"abc\", 2), \" \", clock.add(100, 50))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "4.0 ['ab', 'ac', 'bc'] 150\n"), "{}", stderr);
}

#[test]
fn builtin_module_chart_svg() {
    let dir = std::env::temp_dir().join("globw_chart_test");
    std::fs::create_dir_all(&dir).unwrap();
    let svg = dir.join("c.svg").to_str().unwrap().replace('\\', "/");
    let src = format!("import chart\nchart.line([1.0, 4.0, 2.0, 5.0], \"{}\")\noutput(\"done\")\n", svg);
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-write", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "done\n"), "{}", stderr);
    let out = std::fs::read_to_string(dir.join("c.svg")).unwrap();
    assert!(out.contains("<svg") && out.contains("<polyline"), "{}", out);
}

#[test]
fn json_diagnostics_are_machine_readable() {
    let dir = std::env::temp_dir().join("globw_json_diag_test");
    std::fs::create_dir_all(&dir).unwrap();
    // a valid program reports an empty array
    std::fs::write(dir.join("ok.gw"), "variables:\n    x as int\nx = 5\noutput(x)\n").unwrap();
    let (code, stdout, stderr) = globw(&["--check", "--json", dir.join("ok.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "[]\n"), "{}", stderr);
    // a type error is reported as JSON with a line and a message
    std::fs::write(dir.join("bad.gw"), "variables:\n    x as int\nx = \"hello\"\n").unwrap();
    let (code, stdout, _) = globw(&["--check", "--json", dir.join("bad.gw").to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(stdout.contains("\"line\":3") && stdout.contains("\"message\":"), "{}", stdout);
}

#[test]
fn ffi_memory_roundtrip() {
    // alloc / poke / peek use no external library, so this runs on every OS and
    // proves the struct-building memory API is portable.
    let dir = std::env::temp_dir().join("globw_ffi_mem_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import ffi\nvariables:\n    buf as int\n\
               buf = ffi.alloc(16)\n\
               ffi.poke_int(buf, 0, 42)\n\
               ffi.poke_real(buf, 8, 3.5)\n\
               output(ffi.peek_int(buf, 0), \" \", ffi.peek_real(buf, 8))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-run", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "42 3.5\n"), "{}", stderr);
}

#[test]
fn ffi_calls_c_on_every_os() {
    // the FFI itself is cross-platform (libloading + extern "C"); only the library
    // name differs. sqrt (double -> double) lives in the math library, abs (int ->
    // int) in the C library. abs returns a 32-bit int, so it goes through ffi.int32:
    // on the System V ABI (Linux/macOS) the upper bits of a 64-bit read are undefined.
    let (math, libc) = if cfg!(windows) {
        ("msvcrt.dll", "msvcrt.dll")
    } else if cfg!(target_os = "macos") {
        ("libSystem.dylib", "libSystem.dylib")
    } else {
        ("libm.so.6", "libc.so.6")
    };
    let dir = std::env::temp_dir().join("globw_ffi_xplat_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = format!(
        "import ffi\nvariables:\n    m, c as int\n\
         m = ffi.open(\"{math}\")\n\
         c = ffi.open(\"{libc}\")\n\
         output(ffi.real(m, \"sqrt\", [16.0]), \" \", ffi.int32(c, \"abs\", [-9]))\n"
    );
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-run", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "4.0 9\n"), "{}", stderr);
}

#[cfg(not(feature = "ffi-full"))]
#[test]
fn ffi_call_needs_ffi_full_feature() {
    // the general libffi-backed call is opt-in; without the feature it says so.
    // No library is opened (the error fires before the handle is used), so this
    // runs on every OS.
    let dir = std::env::temp_dir().join("globw_ffi_full_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import ffi\noutput(ffi.call_real(0, \"pow\", \"dd\", [2.0, 10.0]))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, _, stderr) = globw(&["--allow-run", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(stderr.contains("ffi-full"), "{}", stderr);
}

#[cfg(all(windows, feature = "ffi-full"))]
#[test]
fn ffi_call_mixed_signature() {
    // libffi handles a mix of int and float in one signature: ldexp(double, int).
    let dir = std::env::temp_dir().join("globw_ffi_mix_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import ffi\nvariables:\n    lib as int\n\
               lib = ffi.open(\"msvcrt.dll\")\n\
               output(ffi.call_real(lib, \"ldexp\", \"di\", [1.5, 3.0]))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-run", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "12.0\n"), "{}", stderr);
}

#[test]
fn ffi_is_denied_without_allow_run() {
    // the ffi module runs native code, so it needs --allow-run.
    let dir = std::env::temp_dir().join("globw_ffi_perm_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.gw"), "import ffi\nvariables:\n    h as int\nh = ffi.open(\"msvcrt.dll\")\n").unwrap();
    let (code, _, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(code, 2, "{}", stderr);
    assert!(stderr.contains("ffi") && stderr.contains("--allow-run"), "{}", stderr);
}

#[cfg(windows)]
#[test]
fn ffi_calls_a_c_function() {
    // load the C runtime and call double sqrt(double) / int abs(int) through FFI.
    let dir = std::env::temp_dir().join("globw_ffi_call_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import ffi\nvariables:\n    lib as int\n\
               lib = ffi.open(\"msvcrt.dll\")\n\
               output(ffi.real(lib, \"sqrt\", [16.0]), \" \", ffi.int(lib, \"abs\", [-9]))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-run", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "4.0 9\n"), "{}", stderr);
}

#[cfg(windows)]
#[test]
fn ffi_strings_and_buffers() {
    // strings (atoi via a C string), 32-bit int return, and passing a nungw
    // buffer to C (memset zeroes it in place).
    let dir = std::env::temp_dir().join("globw_ffi_str_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import ffi\nimport nungw\nvariables:\n    lib, r as int\n    a as nungw of float\n\
               lib = ffi.open(\"msvcrt.dll\")\n\
               a = nungw.of([1.0, 2.0, 3.0, 4.0])\n\
               r = ffi.int(lib, \"memset\", [ffi.buffer(a), 0, 32])\n\
               output(ffi.int32(lib, \"atoi\", [ffi.cstr(\"100\")]), \" \", nungw.sum(a))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-run", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "100 0.0\n"), "{}", stderr);
}

#[test]
fn permissions_are_denied_by_default() {
    let dir = std::env::temp_dir().join("globw_perm_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.gw"), "import shell\noutput(trim(shell.run(\"echo hi\")))\n").unwrap();
    let p = dir.join("p.gw");
    // refused without the flag
    let (code, _, stderr) = globw(&[p.to_str().unwrap()]);
    assert_eq!(code, 2, "{}", stderr);
    assert!(stderr.contains("not permitted") && stderr.contains("--allow-run"), "{}", stderr);
    // allowed with it
    let (code, stdout, stderr) = globw(&["--allow-run", p.to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "hi\n"), "{}", stderr);
}

#[test]
fn nungw_gpu_roundtrip_and_matmul() {
    // Works in both builds: with the `gpu` feature these run GPU-resident; without
    // it, to_gpu/to_cpu are no-ops and matmul runs on the CPU -- same answer.
    let dir = std::env::temp_dir().join("globw_gpu_rt_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    e, b, eg, bg, rg, r as nungw of float\n\
               e = nungw.eye(4)\n\
               b = nungw.reshape(nungw.range(16), [4, 4])\n\
               eg = nungw.to_gpu(e)\n\
               bg = nungw.to_gpu(b)\n\
               rg = nungw.matmul(eg, bg)\n\
               r = nungw.to_cpu(rg)\n\
               output(nungw.to_list(r))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    // eye(4) * reshape(0..15, 4x4) == the matrix itself
    assert_eq!(
        (code, stdout.as_str()),
        (0, "[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0]\n"),
        "{}",
        stderr
    );
}

#[test]
fn nungw_gpu_resident_matmul_ta() {
    // a^T·b on resident arrays (gpu) or on the CPU (no feature) -- same answer.
    let dir = std::env::temp_dir().join("globw_gpu_ta_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    a, b, ag, bg, r as nungw of float\n\
               a = nungw.reshape(nungw.range(6), [2, 3])\n\
               b = nungw.reshape(nungw.range(8), [2, 4])\n\
               ag = nungw.to_gpu(a)\n\
               bg = nungw.to_gpu(b)\n\
               r = nungw.to_cpu(nungw.matmul_ta(ag, bg))\n\
               output(nungw.to_list(r))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(
        (code, stdout.as_str()),
        (0, "[12.0, 15.0, 18.0, 21.0, 16.0, 21.0, 26.0, 31.0, 20.0, 27.0, 34.0, 41.0]\n"),
        "{}",
        stderr
    );
}

#[test]
fn nungw_gpu_resident_elementwise() {
    // fused element-wise on a resident array stays resident (gpu build) or runs on
    // the CPU (no-op to_gpu without the feature) -- same answer either way.
    let dir = std::env::temp_dir().join("globw_gpu_ew_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    a, g, r as nungw of float\n\
               a = nungw.of([1.0, 2.0, 3.0, 4.0])\n\
               g = nungw.to_gpu(a)\n\
               g = g * 2.0 + g\n\
               r = nungw.to_cpu(g)\n\
               output(nungw.to_list(r))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "[3.0, 6.0, 9.0, 12.0]\n"), "{}", stderr);
}

#[test]
fn nungw_gpu_resident_broadcast_bias() {
    // a bias add z + b broadcasts a row vector over the rows of a resident matrix;
    // the result must match the CPU broadcast (gpu build: resident, else CPU).
    let dir = std::env::temp_dir().join("globw_gpu_bcast_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    z, b, zg, bg, r as nungw of float\n\
               z = nungw.reshape(nungw.range(6), [2, 3])\n\
               b = nungw.of([10.0, 20.0, 30.0])\n\
               zg = nungw.to_gpu(z)\n\
               bg = nungw.to_gpu(b)\n\
               r = zg + bg\n\
               output(nungw.to_list(nungw.to_cpu(r)))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(
        (code, stdout.as_str()),
        (0, "[10.0, 21.0, 32.0, 13.0, 24.0, 35.0]\n"),
        "{}",
        stderr
    );
}

#[test]
fn nungw_gpu_resident_activation() {
    // a fused activation chain (here sign(neg(x)), both unary ops) runs as ONE
    // resident kernel on the GPU; sign is exact in f32, so the result matches the
    // CPU bit-for-bit either way. Proves unary transcendentals/activations fuse.
    let dir = std::env::temp_dir().join("globw_gpu_act_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    x, g, r as nungw of float\n\
               x = nungw.of([-2.0, 1.0, 3.0, -5.0])\n\
               g = nungw.to_gpu(x)\n\
               g = nungw.sign(nungw.neg(g))\n\
               r = nungw.to_cpu(g)\n\
               output(nungw.to_list(r))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "[1.0, -1.0, -1.0, 1.0]\n"), "{}", stderr);
}

#[test]
fn nungw_gpu_resident_axis_reduce() {
    // max/mean/prod along an axis stay resident; integer inputs are exact in f32,
    // so the result matches the CPU either way.
    let dir = std::env::temp_dir().join("globw_gpu_axisred_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    z, g as nungw of float\n\
               z = nungw.reshape(nungw.of([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]), [2, 3])\n\
               g = nungw.to_gpu(z)\n\
               output(nungw.to_list(nungw.to_cpu(nungw.max_axis(g, 0))))\n\
               output(nungw.to_list(nungw.to_cpu(nungw.prod_axis(g, 1))))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "[4.0, 5.0, 6.0]\n[6.0, 120.0]\n"), "{}", stderr);
}

#[test]
fn nungw_gpu_resident_full_reduce() {
    // whole-array sum/max/prod of a resident array: the array stays on the device,
    // only the scalar comes back. Integer values are exact in f32.
    let dir = std::env::temp_dir().join("globw_gpu_fullred_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    a, g as nungw of float\n\
               a = nungw.of([1.0, 2.0, 3.0, 4.0])\n\
               g = nungw.to_gpu(a)\n\
               output(nungw.sum(g), \" \", nungw.max(g), \" \", nungw.prod(g))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "10.0 4.0 24.0\n"), "{}", stderr);
}

#[test]
fn nungw_gpu_resident_binary() {
    // a binary element-wise op between two arrays (maximum) fuses and stays
    // resident; integer inputs are exact in f32. Also exercises resident reshape.
    let dir = std::env::temp_dir().join("globw_gpu_bin_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    a, b, ga, gb, r as nungw of float\n\
               a = nungw.of([1.0, 5.0, 2.0, 7.0])\n\
               b = nungw.of([3.0, 4.0, 6.0, 1.0])\n\
               ga = nungw.to_gpu(a)\n\
               gb = nungw.to_gpu(b)\n\
               r = nungw.reshape(nungw.maximum(ga, gb), [2, 2])\n\
               output(nungw.to_list(nungw.to_cpu(r)))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "[3.0, 5.0, 6.0, 7.0]\n"), "{}", stderr);
}

#[test]
fn nungw_rows_slices_a_matrix() {
    // nungw.rows takes a 1-based inclusive range of rows of a 2-D matrix,
    // keeping the 2-D shape.
    let dir = std::env::temp_dir().join("globw_rows_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    a, r as nungw of float\n\
               a = nungw.reshape(nungw.range(12), [4, 3])\n\
               r = nungw.rows(a, 2, 3)\n\
               output(nungw.to_list(r))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "[3.0, 4.0, 5.0, 6.0, 7.0, 8.0]\n"), "{}", stderr);
}

#[test]
fn nungw_eig_svd_reconstruct() {
    // eigvecs_sym and svd_u/svd_v must reconstruct the matrix:
    // A = V diag(lambda) V^T, and A = U diag(sigma) V^T.
    let dir = std::env::temp_dir().join("globw_eigsvd_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n\
               \x20   asym, vals, vecs, recon as nungw of float\n\
               \x20   ag, sig, u, v, arec as nungw of float\n\
               \x20   errm, errs as float\n\
               asym = nungw.reshape(nungw.of([2.0, 1.0, 1.0, 2.0]), [2, 2])\n\
               vals = nungw.eigvals_sym(asym)\n\
               vecs = nungw.eigvecs_sym(asym)\n\
               recon = nungw.matmul(vecs, nungw.transpose(vecs) * nungw.reshape(vals, [2, 1]))\n\
               errm = nungw.max(nungw.abs(recon - asym))\n\
               ag = nungw.reshape(nungw.of([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]), [3, 2])\n\
               sig = nungw.singular_values(ag)\n\
               u = nungw.svd_u(ag)\n\
               v = nungw.svd_v(ag)\n\
               arec = nungw.matmul(u, nungw.transpose(v) * nungw.reshape(sig, [2, 1]))\n\
               errs = nungw.max(nungw.abs(arec - ag))\n\
               if(errm < 0.000001)\n\
               \x20   if(errs < 0.000001)\n\
               \x20       output(\"ok\")\n\
               \x20   end if\n\
               end if\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "ok\n"), "{}", stderr);
}

#[test]
fn nungw_inplace_reuse_is_correct() {
    // a fused expression reuses a dead temporary's buffer in place; it must give
    // the right result AND must never clobber a live variable (here x).
    let dir = std::env::temp_dir().join("globw_reuse_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = "import nungw\nvariables:\n    x, y, r as nungw of float\n\
               x = nungw.of([1.0, 4.0, 9.0, 16.0])\n\
               y = nungw.of([25.0, 36.0, 49.0, 64.0])\n\
               r = nungw.sqrt(x) + nungw.sqrt(y)\n\
               output(nungw.to_list(r))\n\
               output(nungw.to_list(x))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "[6.0, 8.0, 10.0, 12.0]\n[1.0, 4.0, 9.0, 16.0]\n"), "{}", stderr);
}

#[test]
fn permission_scopes_limit_paths() {
    let dir = std::env::temp_dir().join("globw_scope_test");
    std::fs::create_dir_all(&dir).unwrap();
    let dirs = dir.to_str().unwrap().replace('\\', "/");
    let target = format!("{}/out.txt", dirs);
    std::fs::write(dir.join("p.gw"), format!("write_file(\"{}\", \"hi\")\noutput(\"ok\")\n", target)).unwrap();
    let prog = dir.join("p.gw");
    let prog = prog.to_str().unwrap();
    // granted directory -> allowed
    let (code, stdout, stderr) = globw(&[&format!("--allow-write={}", dirs), prog]);
    assert_eq!((code, stdout.as_str()), (0, "ok\n"), "{}", stderr);
    // a different directory granted -> refused
    let (code, _, stderr) = globw(&["--allow-write=/some/other/dir", prog]);
    assert_eq!(code, 2, "{}", stderr);
    assert!(stderr.contains("not permitted") && stderr.contains("--allow-write"), "{}", stderr);
}

#[test]
fn builtin_module_nungw_matmul() {
    let dir = std::env::temp_dir().join("globw_nungw_matmul_test");
    std::fs::create_dir_all(&dir).unwrap();
    // [[1,2,3],[4,5,6]] x [[7,8],[9,10],[11,12]] = [[58,64],[139,154]]
    // both the explicit form and the shape-aware form must give the same result
    let src = "import nungw\nvariables:\n    a, b as nungw of float\n\
               a = nungw.reshape(nungw.of([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]), [2, 3])\n\
               b = nungw.reshape(nungw.of([7.0, 8.0, 9.0, 10.0, 11.0, 12.0]), [3, 2])\n\
               output(nungw.to_list(nungw.matmul(a, 2, 3, b, 2)))\n\
               output(nungw.to_list(nungw.matmul(a, b)))\n\
               output(nungw.shape(nungw.transpose(a)))\n";
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(
        (code, stdout.as_str()),
        (0, "[58.0, 64.0, 139.0, 154.0]\n[58.0, 64.0, 139.0, 154.0]\n[3, 2]\n"),
        "{}",
        stderr
    );
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

#[test]
fn builtins_catalogue_is_json() {
    // `gw --builtins` prints the builtin catalogue (name + signature) for editors.
    let (code, stdout, stderr) = globw(&["--builtins"]);
    assert_eq!(code, 0, "{}", stderr);
    assert!(stdout.starts_with('['), "should be a JSON array: {}", stdout);
    assert!(stdout.contains("\"name\":\"ffi.open\""), "lists ffi.open");
    assert!(stdout.contains("\"name\":\"shell.run\",\"sig\":\"(string) -> string\""), "renders a fixed signature");
}

/// One raw HTTP/1.1 GET to 127.0.0.1:port, returning the whole response text.
fn http_get(port: u16, path: &str) -> String {
    use std::io::{Read, Write};
    for _ in 0..80 {
        if let Ok(mut c) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
            c.write_all(req.as_bytes()).unwrap();
            let mut out = String::new();
            let _ = c.read_to_string(&mut out);
            return out;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("the Globw server did not start on port {port}");
}

#[test]
fn serve_files_json_and_query() {
    // a dynamic web UI: static files from a folder, a JSON API, and the query
    // string decoded into req.query
    let dir = std::env::temp_dir().join(format!("gw_serve_test_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("www")).unwrap();
    std::fs::write(dir.join("www/index.html"), "<h1>home</h1>").unwrap();
    std::fs::write(dir.join("www/style.css"), "h1{color:red}").unwrap();
    std::fs::write(
        dir.join("app.gw"),
        "import serve\nimport data\nvariables:\n    s as int\n    req as string\n    route as string\n\
         s = serve.start(18765)\nwhile(true)\n    req = serve.wait(s)\n    route = data.text(req, \"route\")\n\
             if(route == \"/api/echo\")\n        serve.reply_as(s, 200, \"application/json\", data.to_json(data.text(req, \"query.msg\")))\n\
             else\n        serve.file(s, \"www\", route)\n    end if\nend loop\n",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_gw"))
        .args(["--allow-net", "--allow-read"])
        .arg(dir.join("app.gw"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let home = http_get(18765, "/");
    let css = http_get(18765, "/style.css");
    let api = http_get(18765, "/api/echo?msg=hi%20there");
    let missing = http_get(18765, "/nope.png");
    let escape = http_get(18765, "/../app.gw");
    let _ = child.kill();
    let _ = child.wait();
    let lower = |s: &str| s.to_ascii_lowercase();
    assert!(home.starts_with("HTTP/1.1 200") && lower(&home).contains("content-type: text/html") && home.contains("<h1>home</h1>"), "{home}");
    assert!(lower(&css).contains("content-type: text/css") && css.contains("h1{color:red}"), "{css}");
    assert!(lower(&api).contains("content-type: application/json") && api.contains("\"hi there\""), "{api}");
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    assert!(escape.starts_with("HTTP/1.1 403"), "{escape}");
}

/// A minimal Debug Adapter Protocol client for `gw --dap`, like an editor.
struct Dap {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    rx: std::sync::mpsc::Receiver<serde_json::Value>,
    seq: i64,
    output: String,
    /// messages read but not asked for yet (events may overtake responses)
    pending: Vec<serde_json::Value>,
}

impl Dap {
    fn start() -> Dap {
        use std::io::{BufRead, Read};
        use std::process::Stdio;
        let mut child = Command::new(env!("CARGO_BIN_EXE_gw"))
            .arg("--dap")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || loop {
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if out.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(v) = line.strip_prefix("Content-Length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0u8; len];
            out.read_exact(&mut body).unwrap();
            if tx.send(serde_json::from_slice(&body).unwrap()).is_err() {
                return;
            }
        });
        Dap { child, stdin, rx, seq: 0, output: String::new(), pending: Vec::new() }
    }

    /// The next message that `pred` accepts (output events are collected on the way).
    fn wait(&mut self, what: &str, pred: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
        if let Some(i) = self.pending.iter().position(&pred) {
            return self.pending.remove(i);
        }
        loop {
            let m = self.rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap_or_else(|_| panic!("no {}", what));
            if m["event"] == "output" {
                self.output.push_str(m["body"]["output"].as_str().unwrap_or(""));
                continue;
            }
            if pred(&m) {
                return m;
            }
            self.pending.push(m);
        }
    }

    fn request(&mut self, command: &str, arguments: serde_json::Value) -> serde_json::Value {
        use std::io::Write;
        self.seq += 1;
        let seq = self.seq;
        let body = serde_json::json!({"seq": seq, "type": "request", "command": command, "arguments": arguments}).to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
        let r = self.wait(command, |m| m["type"] == "response" && m["request_seq"] == seq);
        assert_eq!(r["success"], true, "{} failed: {}", command, r);
        r["body"].clone()
    }

    fn stopped(&mut self) -> serde_json::Value {
        self.wait("stop", |m| m["event"] == "stopped")
    }

    /// The top frame's line and its local variables as "name=value".
    fn top(&mut self) -> (u64, Vec<String>, Vec<String>) {
        let st = self.request("stackTrace", serde_json::json!({"threadId": 1}));
        let frames = st["stackFrames"].as_array().unwrap().clone();
        let names = frames.iter().map(|f| f["name"].as_str().unwrap().to_string()).collect();
        let scopes = self.request("scopes", serde_json::json!({"frameId": frames[0]["id"]}));
        let r = scopes["scopes"][0]["variablesReference"].clone();
        let vars = self.request("variables", serde_json::json!({"variablesReference": r}));
        let vars = vars["variables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| format!("{}={}", v["name"].as_str().unwrap(), v["value"].as_str().unwrap()))
            .collect();
        (frames[0]["line"].as_u64().unwrap(), names, vars)
    }

    fn launch(&mut self, program: &std::path::Path, breakpoints: &[u32]) {
        self.request("initialize", serde_json::json!({"adapterID": "globw"}));
        self.request("launch", serde_json::json!({"program": program}));
        let bps: Vec<_> = breakpoints.iter().map(|l| serde_json::json!({"line": l})).collect();
        self.request("setBreakpoints", serde_json::json!({"source": {"path": program}, "breakpoints": bps}));
        self.request("configurationDone", serde_json::json!({}));
    }

    fn finish(mut self) -> String {
        self.wait("terminated", |m| m["event"] == "terminated");
        self.request("disconnect", serde_json::json!({}));
        let _ = self.child.wait();
        self.output.replace("\r\n", "\n")
    }
}

fn temp_program(name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gw-dap-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}.gw", name));
    std::fs::write(&path, src).unwrap();
    path
}

#[test]
fn debugger_breakpoints_stepping_and_variables() {
    let src = "variables:\n    total, i as int\n\nfunction square(n as int) as int\n    variables:\n        r as int\n    r = n * n\n    return r\nend function\n\ntotal = 0\nfor(i from 1 to 3)\n    total = total + square(i)\nend loop\noutput(\"total \", total)\n";
    let program = temp_program("steps", src);
    let mut dap = Dap::start();
    dap.launch(&program, &[7]);
    // the breakpoint inside square: two frames, the argument is visible
    assert_eq!(dap.stopped()["body"]["reason"], "breakpoint");
    let (line, frames, vars) = dap.top();
    assert_eq!((line, frames), (7, vec!["square".to_string(), "main program".to_string()]));
    assert_eq!(vars, vec!["n=1"]);
    // step over: the next line of square, with r now set
    dap.request("next", serde_json::json!({"threadId": 1}));
    assert_eq!(dap.stopped()["body"]["reason"], "step");
    let (line, _, vars) = dap.top();
    assert_eq!((line, vars), (8, vec!["n=1".to_string(), "r=1".to_string()]));
    // step out: back in the main program, at the call
    dap.request("stepOut", serde_json::json!({"threadId": 1}));
    dap.stopped();
    let (line, frames, _) = dap.top();
    assert_eq!((line, frames.len()), (13, 1));
    // continue: the breakpoint again, on the next round
    dap.request("continue", serde_json::json!({"threadId": 1}));
    dap.stopped();
    assert_eq!(dap.top().2, vec!["n=2"]);
    // clear the breakpoints and run to the end
    dap.request("setBreakpoints", serde_json::json!({"source": {"path": program}, "breakpoints": []}));
    dap.request("continue", serde_json::json!({"threadId": 1}));
    assert_eq!(dap.finish(), "total 14\n");
}

#[test]
fn debugger_stops_on_a_run_time_error() {
    let src = "variables:\n    d, x as int\n\nfunction divide(a as int, b as int) as int\n    return a div b\nend function\n\nd = 0\nx = divide(10, d)\n";
    let program = temp_program("crash", src);
    let mut dap = Dap::start();
    dap.launch(&program, &[]);
    let stop = dap.stopped();
    assert_eq!(stop["body"]["reason"], "exception");
    assert_eq!(stop["body"]["text"], "division by zero");
    let (line, frames, vars) = dap.top();
    assert_eq!((line, frames.len(), vars), (5, 2, vec!["a=10".to_string(), "b=0".to_string()]));
    dap.request("continue", serde_json::json!({"threadId": 1}));
    assert!(dap.finish().contains("division by zero"));
}

#[test]
fn builtin_module_model_obj() {
    let dir = std::env::temp_dir().join("globw_model_test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.obj"), "v 0 0 0\nv 2 0 0\nv 2 1 0\nv 0 1 0\nf 1 2 3 4\n").unwrap();
    let obj = dir.join("t.obj").to_str().unwrap().replace('\\', "/");
    let src = format!(
        "import model\nvariables:\n    m, t as int\n\
         m = model.load(\"{}\")\n\
         t = model.new()\n\
         model.triangle(t, 0, 0, 0, 1, 0, 0, 0, 1, 0, \"red\")\n\
         output(model.triangles(m), \" \", model.width(m), \" \", model.height(m), \" \", model.triangles(t))\n",
        obj
    );
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, stdout, stderr) = globw(&["--allow-read", dir.join("p.gw").to_str().unwrap()]);
    assert_eq!((code, stdout.as_str()), (0, "2 2.0 1.0 1\n"), "{}", stderr);
    // reading a model file needs --allow-read
    let (code, _, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(code, 2, "{}", stderr);
    assert!(stderr.contains("--allow-read"), "{}", stderr);
}

#[test]
fn screenshot_needs_allow_write() {
    let dir = std::env::temp_dir().join("globw_shot_perm_test");
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("s.png");
    let _ = std::fs::remove_file(&png);
    // the permission is checked first, so no window is needed (CI has no screen)
    let src = format!("screenshot(\"{}\")\n", png.to_str().unwrap().replace('\\', "/"));
    std::fs::write(dir.join("p.gw"), src).unwrap();
    let (code, _, stderr) = globw(&[dir.join("p.gw").to_str().unwrap()]);
    assert_eq!(code, 2, "{}", stderr);
    assert!(stderr.contains("--allow-write"), "{}", stderr);
    assert!(!png.exists());
}

#[test]
fn nungw_reads_bytes_plain_and_gzip() {
    let dir = std::env::temp_dir().join("globw_read_bytes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plain.bin"), [9u8, 1, 2, 3, 250, 255]).unwrap();
    // gzip of the bytes 1, 2, 3, 250, 255
    let gz: [u8; 25] = [31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 99, 100, 98, 254, 245, 31, 0, 197, 78, 92, 144, 5, 0, 0, 0];
    std::fs::write(dir.join("packed.gz"), gz).unwrap();
    let plain = dir.join("plain.bin").display().to_string().replace('\\', "/");
    let packed = dir.join("packed.gz").display().to_string().replace('\\', "/");
    let src = format!(
        "import nungw\noutput(nungw.to_list(nungw.read_bytes(\"{}\", 1, 5)))\noutput(nungw.to_list(nungw.read_bytes(\"{}\", 2, 3)))\n",
        plain, packed
    );
    let prog = dir.join("read.gw");
    std::fs::write(&prog, &src).unwrap();
    let prog = prog.display().to_string();
    let (code, stdout, stderr) = globw(&["--allow-read", &prog]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "[1.0, 2.0, 3.0, 250.0, 255.0]\n[3.0, 250.0, 255.0]\n");
    // without --allow-read it is refused
    let (code, _, stderr) = globw(&[&prog]);
    assert_ne!(code, 0);
    assert!(stderr.contains("--allow-read"), "{}", stderr);
    // asking for more bytes than there are
    std::fs::write(dir.join("short.gw"), format!("import nungw\noutput(nungw.size(nungw.read_bytes(\"{}\", 4, 9)))\n", plain)).unwrap();
    let (code, _, stderr) = globw(&["--allow-read", &dir.join("short.gw").display().to_string()]);
    assert_ne!(code, 0);
    assert!(stderr.contains("has 6 bytes"), "{}", stderr);
}

#[test]
fn nungw_for_neural_networks() {
    let dir = std::env::temp_dir().join("globw_nungw_ml");
    std::fs::create_dir_all(&dir).unwrap();
    // a 4 x 3 PNG: (10, 20, 30) everywhere, red top-left, green top-right, blue bottom-right
    let png: [u8; 88] = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 3, 8, 2, 0, 0, 0, 59, 150, 57, 145, 0, 0, 0, 31,
        73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 192, 37, 34, 199, 37, 34, 199, 240, 159, 129, 17, 68, 193, 0, 19, 156, 245, 237, 205,
        67, 0, 92, 84, 5, 121, 82, 37, 34, 35, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    std::fs::write(dir.join("t.png"), png).unwrap();
    let folder = dir.display().to_string().replace('\\', "/");
    let src = format!(
        "import nungw
variables:
    a, b, c, img as nungw of float
a = nungw.reshape(nungw.of([1.0, -2.0, 3.0, 0.5, 4.0, -1.0]), [2, 3])
output(nungw.to_list(nungw.relu(a)))
output(nungw.to_list(nungw.softmax(nungw.of([1000.0, 1000.0]), 0)))
output(round(nungw.sum(nungw.softmax(a, 1)) * 1000.0))
output(nungw.to_list(nungw.argmax_axis(a, 1)), nungw.to_list(nungw.argmax_axis(a, 0)), nungw.to_list(nungw.argmin_axis(a, 1)))
b = nungw.reshape(nungw.range(24), [2, 3, 4]) * 0.1
nungw.save(b, \"{0}/b.gwng\")
c = nungw.load(\"{0}/b.gwng\")
output(nungw.shape(c), nungw.sum(nungw.abs(c - b)) == 0.0)
img = nungw.read_image(\"{0}/t.png\")
output(nungw.shape(img), nungw.at(img, 1, 1, 1), nungw.at(img, 1, 4, 2), nungw.at(img, 3, 4, 3), nungw.at(img, 2, 2, 3))
",
        folder
    );
    let prog = dir.join("ml.gw");
    std::fs::write(&prog, src).unwrap();
    let prog = prog.display().to_string();
    let (code, stdout, stderr) = globw(&["--allow-read", "--allow-write", &prog]);
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(
        stdout,
        "[1.0, 0.0, 3.0, 0.5, 4.0, 0.0]\n[0.5, 0.5]\n2000\n[3.0, 2.0][1.0, 2.0, 1.0][2.0, 3.0]\n[2, 3, 4]true\n[3, 4, 3]255.0255.0255.030.0\n"
    );
    // saving needs --allow-write; a file that is not nungw's is refused
    let (code, _, stderr) = globw(&["--allow-read", &prog]);
    assert_ne!(code, 0);
    assert!(stderr.contains("--allow-write"), "{}", stderr);
    std::fs::write(dir.join("load.gw"), format!("import nungw\noutput(nungw.size(nungw.load(\"{}/t.png\")))\n", folder)).unwrap();
    let (code, _, stderr) = globw(&["--allow-read", &dir.join("load.gw").display().to_string()]);
    assert_ne!(code, 0);
    assert!(stderr.contains("not a file made by nungw.save"), "{}", stderr);
}
