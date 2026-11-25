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
    if jit {
        cmd.env("GLOBW_JIT", "1");
    } else {
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

#[test]
fn while_loops_arrays_and_matrices() {
    let src = "variables:
    a as int[300]
    m as float[20, 20]
    i, j, k, t, n as int
    total as float
n = 300
for(i from 1 to n)
    a[i] = (i * 7919) mod 1000
end loop
' bubble sort, with while loops
i = 1
while(i < n)
    j = 1
    while(j <= n - i)
        if(a[j] > a[j + 1])
            t = a[j]
            a[j] = a[j + 1]
            a[j + 1] = t
        end if
        j = j + 1
    end loop
    i = i + 1
end loop
for(i from 1 to 20)
    for(j from 1 to 20)
        m[i, j] = i * 1.5 - j
    end loop
end loop
total = 0.0
for(k from 1 to 100)
    for(i from 1 to 20)
        total = total + m[i, (i + k) mod 20 + 1]
    end loop
end loop
output(a[1], \" \", a[150], \" \", a[300], \" \", length(a), \" \", total)
";
    let (code, out, _) = same("arrays", src);
    assert_eq!(code, 0, "{}", out);
}

#[test]
fn lists_text_and_globals_in_compiled_loops() {
    let src = "global variables:
    hits as int
variables:
    words, copy as list of string
    nums as list of int
    i as int
    w, last as string
procedure tally()
    variables:
        k as int
    for(k from 1 to 200)
        hits = hits + 1
    end loop
end procedure
hits = 0
words = [\"red\", \"green\", \"blue\"]
copy = []
nums = []
last = \"\"
for(i from 1 to 300)
    w = words[i mod 3 + 1]
    last = w
    append(copy, w)
    append(nums, i * i)
end loop
tally()
tally()
output(last, \" \", length(copy), \" \", copy[299], \" \", nums[300], \" \", hits)
";
    let (code, out, _) = same("lists", src);
    assert_eq!(code, 0, "{}", out);
    assert!(out.starts_with("red 300 blue 90000 400"), "{}", out);
}

#[test]
fn index_errors_inside_compiled_loops() {
    let src = "variables:
    a as int[100]
    i, s as int
    problem as string
for(i from 1 to 100)
    a[i] = i
end loop
s = 0
try
    for(i from 1 to 200)
        s = s + a[i]
    end loop
catch problem
    output(\"caught: \", problem, \" at i = \", i, \", s = \", s)
end try
";
    let (code, out, _) = same("index", src);
    assert_eq!(code, 0, "{}", out);
    assert!(out.contains("caught: index 101"), "{}", out);
}

#[test]
fn compact_arrays_behave_like_arrays() {
    let src = "variables:
    a as int[200]
    f as float[200]
    flags as bool[200]
    i, s as int
    problem as string
procedure fill(xs as int[])
    variables:
        k as int
    for(k from 1 to 200)
        xs[k] = (k * 37) mod 101
    end loop
end procedure
fill(a)
for(i from 1 to 200)
    f[i] = i
    flags[i] = i mod 3 == 0
end loop
sort(a)
s = 0
for(i from 1 to 200)
    if(flags[i])
        s = s + a[i]
    end if
end loop
output(s, \" \", a[1], \" \", a[200], \" \", f[200] / 3, \" \", flags[3], \" \", length(a))
output(sum(f))
try
    output(a[201])
catch problem
    output(problem)
end try
";
    let (code, out, _) = same("packed", src);
    assert_eq!(code, 0, "{}", out);
    let unset = "variables:
    a as int[100]
    i, s as int
for(i from 1 to 99)
    a[i] = i
end loop
s = 0
for(i from 1 to 100)
    s = s + a[i]
end loop
output(s)
";
    let (code, _, err) = same("packed_unset", unset);
    assert_ne!(code, 0);
    assert!(err.contains("no value"), "{}", err);
}

#[test]
fn library_calls_text_and_output_in_compiled_code() {
    let src = "variables:
    i, n, bad as int
    s, problem as string
n = 0
bad = 0
for(i from 1 to 300)
    s = \"n\" + to_text(i)
    if(i mod 100 == 0)
        output(s, \" \", uppercase(s), \" \", length(s), \" \", s[2])
    end if
    try
        n = n + to_int(substring(s, 2, length(s) - 1)) + round(sqrt(i * 1.0))
        if(i mod 150 == 0)
            n = n + to_int(\"x\" + s)
        end if
    catch problem
        bad = bad + 1
        output(\"caught: \", problem)
    end try
end loop
output(n, \" \", bad)
";
    let (code, out, _) = same("library", src);
    assert_eq!(code, 0, "{}", out);
    assert!(out.contains("caught: to_int: \"xn150\"") && out.ends_with(" 11
"), "{}", out);
}

/// Runs a program with the parallel loops checked: they must run in parallel
/// (the JIT log says so) and give exactly the interpreter's output and errors.
fn same_parallel(name: &str, src: &str) -> (i32, String, String) {
    let r = same(name, src);
    let dir = std::env::temp_dir().join("globw_jit_tests");
    let out = Command::new(env!("CARGO_BIN_EXE_gw"))
        .arg(dir.join(format!("{}.gw", name)))
        .env("GLOBW_JIT", "1")
        .env("GLOBW_JIT_LOG", "1")
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(log.contains("runs in parallel"), "{}: no parallel loop\n{}", name, log);
    r
}

#[test]
fn parallel_sums_conditions_and_the_counter_after() {
    let src = "variables:
    i, s, c, n as int
    flags as bool[2000000]
n = 2000000
for(i from 1 to n)
    flags[i] = i mod 3 == 0
end loop
s = 0
c = 7
for(i from 1 to n)
    s = s + i mod 11 - 5
    if(flags[i])
        c = c + 1
    end if
end loop
output(s, \" \", c, \" \", i)
";
    let (code, out, _) = same_parallel("par_sums", src);
    assert_eq!(code, 0, "{}", out);
}

#[test]
fn parallel_nested_loops_with_floats() {
    let src = "variables:
    px, py, i, total as int
    zr, zi, t, cr, ci as float
total = 0
for(py from 0 to 299)
    for(px from 0 to 299)
        cr = px * 3.5 / 300 - 2.5
        ci = py * 2.0 / 300 - 1.0
        zr = 0.0
        zi = 0.0
        i = 0
        while(i < 100 and zr * zr + zi * zi <= 4.0)
            t = zr * zr - zi * zi + cr
            zi = 2.0 * zr * zi + ci
            zr = t
            i = i + 1
        end loop
        total = total + i
    end loop
end loop
output(total)
";
    let (code, out, _) = same_parallel("par_nested", src);
    assert_eq!(code, 0, "{}", out);
}

#[test]
fn parallel_overflow_only_in_sequential_order() {
    // the running sum overflows at turn 600000 although the total fits: the
    // interpreter's error, at the same line
    let src = "variables:
    i, s as int
s = 0
for(i from 1 to 1000000)
    if(i == 300000 or i == 600000)
        s = s + 5000000000000000000
    end if
    if(i == 700000)
        s = s - 9000000000000000000
    end if
end loop
output(s)
";
    let (code, _, err) = same_parallel("par_overflow", src);
    assert_ne!(code, 0);
    assert!(err.contains("line 6") || err.contains(":6"), "{}", err);
}

#[test]
fn parallel_errors_come_from_the_right_turn() {
    for (name, body) in [
        ("par_div0", "    s = s + 100 div (i - 777777)\n"),
        ("par_index", "    s = s + a[i mod 1000 + 1 + i div 999999]\n"),
    ] {
        let src = format!(
            "variables:
    i, s as int
    a as int[1000]
for(i from 1 to 1000)
    a[i] = i
end loop
s = 0
for(i from 1 to 1000000)
{}end loop
output(s)
",
            body
        );
        let (code, _, err) = same_parallel(name, &src);
        assert_ne!(code, 0, "{}: {}", name, err);
    }
}

#[test]
fn parallel_writes_fill_compact_arrays() {
    let src = "variables:
    i, s as int
    a as int[1000003]
    r as float[1000003]
    b as bool[1000003]
for(i from 1 to 1000003)
    a[i] = i * 3 - 7
    r[i] = i / 4.0
    b[i] = i mod 5 == 2
end loop
s = 0
for(i from 1 to 1000003)
    if(b[i])
        s = s + a[i]
    end if
end loop
output(s, \" \", r[1000003], \" \", a[64], \" \", a[65], \" \", i)
";
    let (code, out, _) = same_parallel("par_fill", src);
    assert_eq!(code, 0, "{}", out);
}

#[test]
fn parallel_writes_and_errors() {
    // an error mid-loop: the interpreter's error, at the same line
    let src = "variables:
    i as int
    a as int[1000000]
for(i from 1 to 1000000)
    a[i] = 1000 div (i - 654321)
end loop
output(a[1])
";
    let (code, _, err) = same_parallel("par_write_err", src);
    assert_ne!(code, 0);
    assert!(err.contains("division by zero"), "{}", err);
    // inside try: the error is caught and the array seen afterwards, so the loop
    // must run in order
    let src = "variables:
    i as int
    a as int[1000000]
    problem as string
for(i from 1 to 1000000)
    a[i] = 0
end loop
try
    for(i from 1 to 1000000)
        a[i] = 1000 div (i - 654321)
    end loop
catch problem
    output(problem)
end try
output(a[654320], \" \", a[654322], \" \", a[999999])
";
    let (code, out, _) = same("par_write_try", src);
    assert_eq!(code, 0, "{}", out);
    assert!(out.trim_end().ends_with("-1000 0 0"), "{}", out);
}

#[test]
fn parallel_writes_never_through_an_alias() {
    // the same array as two parameters: writing one is reading the other
    let src = "variables:
    i as int
    a as int[1000000]
procedure shift(x as int[], y as int[])
    variables:
        k as int
    for(k from 1 to 999999)
        x[k] = y[k + 1] * 2
    end loop
end procedure
for(i from 1 to 1000000)
    a[i] = i
end loop
shift(a, a)
output(a[1], \" \", a[500000], \" \", a[999999], \" \", a[1000000])
";
    let (code, out, _) = same("par_alias", src);
    assert_eq!(code, 0, "{}", out);
    assert!(out.starts_with("4 1000002 2000000 1000000"), "{}", out);
}

#[test]
fn full_compact_arrays_and_gaps() {
    let src = "variables:
    i, k, s as int
    a as int[300000]
    r as float[300000]
    g as int[300000]
for(i from 1 to 300000)
    a[i] = 300000 - i
    r[i] = i * 0.5
end loop
s = 0
for(k from 1 to 3)
    for(i from 1 to 300000)
        s = s + a[i] + round(r[i])
        a[i] = a[i] + 1
    end loop
end loop
sort(a)
output(s, \" \", a[1], \" \", a[300000])
for(i from 1 to 299999)
    g[i] = i
end loop
s = 0
for(i from 1 to 300000)
    s = s + g[i]
end loop
output(s)
";
    let (code, out, err) = same("full_arrays", src);
    assert_ne!(code, 0, "{}", out);
    assert!(err.contains("no value") || err.contains("value yet"), "{}", err);
}

#[test]
fn remainders_kept_as_counters() {
    let src = "variables:
    i, k, s, t as int
s = 0
for(k from 1 to 3)
    for(i from -20 to 3000)
        s = s + i mod 7 + (i * 13) mod 5
    end loop
    for(i from 0 to 300000)
        s = s + i mod 7 + (i * 7919) mod 1000 - (i * 3) mod 1
        if(i mod 3 == 0)
            s = s + i mod 11
        end if
    end loop
    t = 0
    for(i from 5 to 99999)
        t = t + (i * 2147483647) mod 1000
    end loop
    s = s + t
end loop
output(s)
";
    let (code, out, _) = same("remainders", src);
    assert_eq!(code, 0, "{}", out);
}

#[test]
fn monotone_sums_still_find_overflow() {
    for (name, start, body) in [
        ("mono_big", "0", "    s = s + 4000000000000000\n"),
        ("mono_edge", "9223372036854775000", "    s = s + i mod 7\n"),
    ] {
        let src = format!(
            "variables:
    i, s as int
s = {}
for(i from 1 to 1000000)
{}end loop
output(s)
",
            start, body
        );
        let (code, _, err) = same_parallel(name, &src);
        assert_ne!(code, 0, "{}: {}", name, err);
    }
}

#[test]
fn bounded_counters_near_the_limit() {
    let src = "variables:
    i, k, hits as int
    big as bool[100000]
for(i from 1 to 100000)
    big[i] = i mod 3 == 0
end loop
hits = 0
for(k from 1 to 5)
    for(i from 1 to 100000)
        if(big[i])
            hits = hits + 2
        end if
    end loop
end loop
output(hits)
hits = 9223372036854775000
for(i from 1 to 100000)
    if(big[i])
        hits = hits + 1
    end if
end loop
output(hits)
";
    let (code, out, err) = same("bounded_counters", src);
    assert_ne!(code, 0, "{}", out);
    assert!(out.starts_with("333330"), "{}", out);
    assert!(err.contains("integer limits"), "{}", err);
}
