//! The JIT must change nothing but speed: each program here runs with the JIT and
//! with the interpreter only (GLOBW_NO_JIT), and the output, the errors and the exit
//! code must be the same. The programs are hot enough to be compiled (loops of 64
//! turns or more, functions called more than once) and go through the corner cases:
//! errors inside compiled code, deep recursion, NaN, counters that overflow.

use std::process::Command;

fn run(src: &str, name: &str, jit: bool) -> (i32, String, String) {
    let dir = std::env::temp_dir().join("globw_jit_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}.gw", name));
    std::fs::write(&path, src).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gw"));
    cmd.arg(&path);
    if !jit {
        cmd.env("GLOBW_NO_JIT", "1");
    }
    let out = cmd.output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

/// Runs both ways, checks they agree, and returns the JIT run.
fn same(name: &str, src: &str) -> (i32, String, String) {
    let a = run(src, name, true);
    let b = run(src, name, false);
    assert_eq!(a, b, "{}: the JIT and the interpreter disagree", name);
    a
}

#[test]
fn hot_loops_and_functions_give_the_same_results() {
    let src = "variables:
    i, s, n as int
    x, y as float
    flag as bool
function fib(k as int) as int
    if(k < 2)
        return k
    end if
    return fib(k - 1) + fib(k - 2)
end function
function half(v as float) as float
    return v / 2.0
end function
function even(k as int) as bool
    return k mod 2 == 0
end function
s = 0
x = 0.0
flag = false
for(i from 1 to 5000)
    s = s + i mod 7 - (i div 3)
    x = x + half(i * 1.5)
    if(even(i))
        flag = not flag
    end if
end loop
output(s, \" \", x, \" \", flag, \" \", fib(20))
n = 0
for(i from 100 to -100 step -3)
    n = n + i
end loop
output(n)
";
    let (code, out, _) = same("basics", src);
    assert_eq!(code, 0);
    assert!(out.contains("6765"), "{}", out);
}

#[test]
fn negative_division_and_remainder_truncate() {
    let src = "variables:
    i, a, b as int
a = 0
b = 0
for(i from -200 to 200)
    a = a + i div 7
    b = b + i mod 7
end loop
output(a, \" \", b, \" \", -7 div 2, \" \", -7 mod 3)
";
    same("negdiv", src);
}

#[test]
fn overflow_in_a_hot_loop_is_the_same_error() {
    let src = "variables:
    i, s as int
s = 9223372036854775000
for(i from 1 to 1000)
    s = s + 100
end loop
output(s)
";
    let (code, _, err) = same("overflow", src);
    assert_ne!(code, 0);
    assert!(err.contains("line 5"), "{}", err);
}

#[test]
fn errors_in_compiled_code_are_caught_by_try() {
    let src = "variables:
    i, s, k as int
    problem as string
function inv(d as int) as int
    return 1000 div d
end function
s = 0
for(k from 1 to 3)
    try
        for(i from 200 to -200 step -1)
            s = s + inv(i)
        end loop
    catch problem
        output(\"caught: \", problem)
    end try
end loop
output(s)
";
    let (code, out, _) = same("trycatch", src);
    assert_eq!(code, 0);
    assert_eq!(out.matches("caught").count(), 3, "{}", out);
}

#[test]
fn zero_step_and_min_div_minus_one() {
    let step0 = "variables:
    i, st as int
st = 0
for(i from 1 to 100 step st)
    output(i)
end loop
";
    let (code, _, err) = same("step0", step0);
    assert_ne!(code, 0);
    assert!(err.contains("step"), "{}", err);
    let mindiv = "variables:
    i, m, s as int
m = -9223372036854775807 - 1
s = 0
for(i from 1 to 100)
    s = s + m div (i - 101)
end loop
output(s)
";
    same("mindiv", mindiv);
}

#[test]
fn deep_and_endless_recursion() {
    let deep = "variables:
    r as int
function down(n as int) as int
    if(n == 0)
        return 0
    end if
    return 1 + down(n - 1)
end function
r = down(10) + down(30000)
output(r)
";
    let (code, out, _) = same("deep", deep);
    assert_eq!((code, out.as_str()), (0, "30010\n"));
    let endless = "variables:
    r as int
function f(n as int) as int
    return f(n + 1) + 1
end function
r = f(0) + f(1)
output(r)
";
    let (code, _, err) = same("endless", endless);
    assert_ne!(code, 0);
    assert!(err.contains("too many nested calls"), "{}", err);
}

#[test]
fn nan_and_infinity_compare_like_the_interpreter() {
    let src = "variables:
    i, lt, ne, eq as int
    big, inf, nan as float
big = 1.0e308
inf = 0.0
lt = 0
ne = 0
eq = 0
for(i from 1 to 200)
    inf = big * 10.0
    nan = inf - inf
    if(nan < 1.0)
        lt = lt + 1
    end if
    if(nan != nan)
        ne = ne + 1
    end if
    if(nan == nan)
        eq = eq + 1
    end if
end loop
output(lt, \" \", ne, \" \", eq, \" \", inf)
";
    let (_, out, _) = same("nan", src);
    assert!(out.starts_with("0 200 0"), "{}", out);
}

#[test]
fn a_counter_that_overflows_ends_the_loop() {
    let src = "variables:
    i, n as int
n = 0
for(i from 9223372036854775000 to 9223372036854775807 step 7)
    n = n + 1
end loop
output(n, \" \", i)
";
    same("counter", src);
}

#[test]
fn registers_holding_text_survive_a_compiled_loop() {
    let src = "variables:
    i, s as int
    name as string
    items as list of string
name = \"globw\"
items = [\"a\", \"b\"]
s = 0
for(i from 1 to 1000)
    s = s + i
end loop
output(name, \" \", s, \" \", length(items))
for(i from 1 to 100)
    if(i > 1000)
        name = \"never\"
    end if
end loop
output(name)
";
    let (_, out, _) = same("text", src);
    assert!(out.contains("globw 500500 2\nglobw"), "{}", out);
}
