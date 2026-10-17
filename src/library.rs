//! The standard library: conversions, math, strings, collections, files and
//! system functions, with Python-like behavior and English names.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use serde_json::Value as Json;

use crate::builtins::{Builtin, CheckError, Checked};
use crate::tir::BaseType::{Bool, Int, Real, Str};
use crate::tir::{fits, join, list_elem, list_of, resolve, BaseType, Type};
use crate::vm::{display, format_real, missing_key, Dict, Key as DictKey, Value};

// ---------------------------------------------------------------- types

/// Element type of an array or list argument.
fn collection(t: Type) -> Option<BaseType> {
    match t {
        Type::Array(e) | Type::List(e) => e,
        _ => None,
    }
}

fn numeric(b: BaseType) -> bool {
    matches!(b, Int | Real)
}

fn ok(convert: Vec<Option<BaseType>>, ret: Type) -> Result<Checked, CheckError> {
    Ok(Checked { convert, ret })
}

fn other<T>(msg: impl Into<String>) -> Result<T, CheckError> {
    Err(CheckError::Other(msg.into()))
}

/// Type rules of the built-ins that accept more than one combination of types.
pub fn check(b: Builtin, a: &[Type]) -> Result<Checked, CheckError> {
    use Builtin::*;
    let none = |n: usize| vec![None; n];
    if a.contains(&Type::Unknown) {
        // an argument already has an error: accept the call quietly
        return ok(none(a.len()), Type::Unknown);
    }
    let arity = |n: usize| if a.len() == n { Ok(()) } else { Err(CheckError::Arity(n)) };
    match b {
        ToInt => {
            arity(1)?;
            match a[0] {
                Type::Base(_) => ok(none(1), Type::Base(Int)),
                _ => other("to_int works with a string, a number or a bool"),
            }
        }
        ToFloat => {
            arity(1)?;
            match a[0] {
                Type::Base(Str | Int | Real) => ok(none(1), Type::Base(Real)),
                _ => other("to_float works with a string or a number"),
            }
        }
        ToText => {
            arity(1)?;
            match a[0] {
                _ => ok(none(1), Type::Base(Str)),
            }
        }
        Absolute => {
            arity(1)?;
            match a[0] {
                Type::Base(t) if numeric(t) => ok(none(1), Type::Base(t)),
                _ => other("absolute works with a number"),
            }
        }
        Minimum | Maximum => match a {
            [Type::Base(x), Type::Base(y)] if numeric(*x) && numeric(*y) => {
                let t = if *x == Int && *y == Int { Int } else { Real };
                ok(vec![Some(t), Some(t)], Type::Base(t))
            }
            [c] if collection(*c).is_some_and(|e| e != Bool) => ok(none(1), Type::Base(collection(*c).unwrap())),
            _ => other(format!("{} works with two numbers, or with an array or list", b.name())),
        },
        Logarithm => match a {
            [Type::Base(x)] if numeric(*x) => ok(vec![Some(Real)], Type::Base(Real)),
            [Type::Base(x), Type::Base(y)] if numeric(*x) && numeric(*y) => {
                ok(vec![Some(Real), Some(Real)], Type::Base(Real))
            }
            _ => other(format!("logarithm takes 1 or 2 numbers, here it gets {} value(s)", a.len())),
        },
        Split => {
            arity(2)?;
            for (index, t) in a.iter().enumerate() {
                if *t != Type::Base(Str) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Str) });
                }
            }
            ok(none(2), Type::List(Some(Str)))
        }
        ReadLines => {
            arity(1)?;
            if a[0] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Str) });
            }
            ok(none(1), Type::List(Some(Str)))
        }
        Join => {
            arity(2)?;
            if collection(a[0]).is_none() {
                return other("join works with an array or list and a separator string");
            }
            if a[1] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Str) });
            }
            ok(none(2), Type::Base(Str))
        }
        Add | Discard => {
            arity(2)?;
            let Type::Set(e) = a[0] else { return other(format!("{} works with a set", b.name())) };
            if !fits(Type::Base(e), a[1]) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(e) });
            }
            ok(vec![None, Some(e)], Type::Unknown)
        }
        ToList => {
            arity(1)?;
            match a[0] {
                Type::Set(e) => ok(none(1), Type::List(Some(e))),
                _ => other("to_list works with a set"),
            }
        }
        Contains | IndexOf => {
            arity(2)?;
            let ret = Type::Base(if b == Contains { Bool } else { Int });
            match (a[0], a[1]) {
                (Type::Set(e), Type::Base(v)) if b == Contains && join(e, v) == Some(e) => ok(vec![None, Some(e)], ret),
                (Type::Base(Str), Type::Base(Str)) if b == Contains => ok(none(2), ret),
                (c, Type::Base(v)) if collection(c).is_some() => {
                    let e = collection(c).unwrap();
                    if join(e, v) == Some(e) {
                        ok(vec![None, Some(e)], ret)
                    } else {
                        other(format!("the value to look for must be {}, here it is {}", e.name(), v.name()))
                    }
                }
                _ if b == Contains => other("contains works with a string and a part, or with an array or list and a value"),
                _ => other("index_of works with an array or list and a value"),
            }
        }
        Sum => {
            arity(1)?;
            match collection(a[0]) {
                Some(e) if numeric(e) => ok(none(1), Type::Base(e)),
                _ => other("sum works with an array or list of numbers"),
            }
        }
        Sort | Reverse => {
            arity(1)?;
            match (collection(a[0]), list_elem(a[0])) {
                (Some(_), _) => ok(none(1), Type::Unknown),
                (None, Some(_)) if b == Reverse => ok(none(1), Type::Unknown),
                (None, Some(_)) => other("sort works only with lists of simple values (ints, floats, strings or bools)"),
                _ => other(format!("{} works with an array or list", b.name())),
            }
        }
        Insert => {
            arity(3)?;
            let Some(e) = list_elem(a[0]) else { return other("insert works with a list") };
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            if !fits(e, a[2]) {
                return other(format!("the value to insert must be {}, here it is {}", e.full_name(), a[2].full_name()));
            }
            ok(vec![None, None, e.base()], Type::Unknown)
        }
        TakeLast | ClearList => {
            arity(1)?;
            match list_elem(a[0]) {
                Some(e) => ok(none(1), if b == TakeLast { e } else { Type::Unknown }),
                None => other(format!("{} works with a list", b.name())),
            }
        }
        HasKey | Get | RemoveKey => {
            arity(if b == Get { 3 } else { 2 })?;
            let Type::Dict(k, v) = a[0] else { return other(format!("{} works with a dictionary", b.name())) };
            if !fits(Type::Base(k), a[1]) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(k) });
            }
            let v = resolve(v);
            match b {
                HasKey => ok(none(2), Type::Base(Bool)),
                RemoveKey => ok(none(2), Type::Unknown),
                _ if fits(v, a[2]) => ok(vec![None, None, v.base()], v),
                _ => Err(CheckError::Arg { index: 2, expected: v }),
            }
        }
        Keys | Values => {
            arity(1)?;
            match a[0] {
                Type::Dict(k, _) if b == Keys => ok(none(1), Type::List(Some(k))),
                Type::Dict(_, v) => ok(none(1), list_of(resolve(v))),
                _ => other(format!("{} works with a dictionary", b.name())),
            }
        }
        TextAll => {
            arity(2)?;
            match (a[0], a[1]) {
                (Type::Base(Str), Type::Base(Str)) => ok(none(2), Type::List(Some(Str))),
                _ => other("text.all needs a string and a pattern"),
            }
        }
        DataKeys => {
            arity(2)?;
            for (index, t) in a.iter().enumerate() {
                if *t != Type::Base(Str) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Str) });
                }
            }
            ok(none(2), Type::List(Some(Str)))
        }
        DataToJson => {
            arity(1)?;
            // any value can be turned into JSON; a bad one fails at run time
            ok(none(1), Type::Base(Str))
        }
        DirList | CsvFields => {
            arity(1)?;
            if a[0] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Str) });
            }
            ok(none(1), Type::List(Some(Str)))
        }
        CsvLine => {
            arity(1)?;
            match collection(a[0]) {
                Some(Str) => ok(none(1), Type::Base(Str)),
                _ => other("csv.line works with an array or list of strings"),
            }
        }
        StatsMean | StatsMedian | StatsStdev => {
            arity(1)?;
            match collection(a[0]) {
                Some(e) if numeric(e) => ok(none(1), Type::Base(Real)),
                _ => other(format!("{} works with an array or list of numbers", b.name())),
            }
        }
        other_builtin => panic!("internal error: {:?} has a fixed signature", other_builtin),
    }
}

// ---------------------------------------------------------------- values

fn int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        other => panic!("internal error: expected an int, found {:?}", other),
    }
}

fn real(v: &Value) -> f64 {
    match v {
        Value::Real(r) => *r,
        Value::Int(n) => *n as f64,
        other => panic!("internal error: expected a number, found {:?}", other),
    }
}

fn text(v: &Value) -> &str {
    match v {
        Value::Str(s) => s,
        other => panic!("internal error: expected a string, found {:?}", other),
    }
}

fn set_data(v: &Value) -> &Rc<RefCell<Dict>> {
    match v {
        Value::Set(d) => d,
        other => panic!("internal error: expected a set, found {:?}", other),
    }
}

fn dict(v: &Value) -> &Rc<RefCell<Dict>> {
    match v {
        Value::Dict(d) => d,
        other => panic!("internal error: expected a dictionary, found {:?}", other),
    }
}

fn new_list(items: Vec<Value>) -> Value {
    Value::Seq(Rc::new(RefCell::new(items)))
}

fn seq(v: &Value) -> &Rc<RefCell<Vec<Value>>> {
    match v {
        Value::Seq(s) => s,
        other => panic!("internal error: expected an array or list, found {:?}", other),
    }
}

fn string(s: impl Into<Rc<str>>) -> Value {
    Value::Str(s.into())
}

/// Compiles a regular expression, turning a bad pattern into a run-time error.
fn pattern(v: &Value) -> Result<regex::Regex, String> {
    regex::Regex::new(text(v)).map_err(|e| format!("invalid pattern: {}", e.to_string().lines().last().unwrap_or("")))
}

/// Lowercase hexadecimal text of a byte sequence (used by the `hash` module).
fn hex(bytes: impl AsRef<[u8]>) -> String {
    let mut s = String::with_capacity(bytes.as_ref().len() * 2);
    for b in bytes.as_ref() {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn list(items: Vec<Value>) -> Value {
    Value::Seq(Rc::new(RefCell::new(items)))
}

/// Fetches a web page over HTTP(S) and returns its body as text (the `web` module).
/// Network, server and timeout problems become ordinary run-time errors, so the
/// program can handle them with `try`/`catch`.
fn web_get(url: &str) -> Result<String, String> {
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("web.get: the server answered with status {}", code),
            ureq::Error::Transport(t) => format!("web.get: cannot reach \"{}\": {}", url, t),
        })?;
    response.into_string().map_err(|e| format!("web.get: cannot read the reply: {}", e))
}

// ---------------------------------------------------------------- JSON (the "data" module)

/// Parses the JSON text of a `data.*` call's first argument.
fn json_root(v: &Value) -> Result<Json, String> {
    serde_json::from_str(text(v)).map_err(|e| format!("invalid JSON: {}", e))
}

/// Follows a path like `users[1].name` into a JSON value. Object members are
/// written `.name` (or just `name` at the start), array positions `[n]` and are
/// 1-based, as everywhere in Globw. An empty path is the whole value.
fn json_at<'a>(root: &'a Json, path: &str) -> Result<&'a Json, String> {
    let mut cur = root;
    let mut rest = path.trim();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('.') {
            rest = r;
        } else if let Some(r) = rest.strip_prefix('[') {
            let end = r.find(']').ok_or_else(|| format!("bad path \"{}\": a \"]\" is missing", path))?;
            let n: usize = r[..end]
                .trim()
                .parse()
                .map_err(|_| format!("bad path \"{}\": an array position must be a whole number", path))?;
            if n < 1 {
                return Err(format!("bad path \"{}\": array positions start at 1", path));
            }
            cur = cur.get(n - 1).ok_or_else(|| format!("the JSON has no value at \"{}\"", path))?;
            rest = &r[end + 1..];
        } else {
            let end = rest.find(|c| c == '.' || c == '[').unwrap_or(rest.len());
            cur = cur.get(&rest[..end]).ok_or_else(|| format!("the JSON has no value at \"{}\"", path))?;
            rest = &rest[end..];
        }
    }
    Ok(cur)
}

/// Turns a Globw value into JSON (for `data.to_json`). Functions, objects and
/// matrices have no JSON form and raise an error.
fn to_json_value(v: &Value) -> Result<Json, String> {
    Ok(match v {
        Value::Null => Json::Null,
        Value::Int(n) => Json::from(*n),
        Value::Real(r) => serde_json::Number::from_f64(*r)
            .map(Json::Number)
            .ok_or("cannot turn an infinite or not-a-number value into JSON")?,
        Value::Bool(b) => Json::Bool(*b),
        Value::Str(s) => Json::String(s.to_string()),
        Value::Seq(items) => Json::Array(items.borrow().iter().map(to_json_value).collect::<Result<_, _>>()?),
        Value::Set(d) => Json::Array(d.borrow().keys().map(|k| to_json_value(&k.value())).collect::<Result<_, _>>()?),
        Value::Dict(d) => {
            let d = d.borrow();
            let mut map = serde_json::Map::new();
            for (k, val) in d.keys().zip(d.values()) {
                let key = match k {
                    DictKey::Int(n) => n.to_string(),
                    DictKey::Str(s) => s.to_string(),
                };
                map.insert(key, to_json_value(val)?);
            }
            Json::Object(map)
        }
        Value::Matrix(_) | Value::Object(_) | Value::Func(_) => {
            return Err("this value has no JSON form".to_string());
        }
    })
}

// ---------------------------------------------------------------- dates and times (the "clock" module)

/// Formats a Unix timestamp (seconds) as local time, using a date pattern
/// like "%Y-%m-%d %H:%M:%S". A bad pattern is a run-time error, not a crash.
fn clock_format(stamp: i64, pattern: &str) -> Result<String, String> {
    use chrono::format::{Item, StrftimeItems};
    let utc = chrono::DateTime::from_timestamp(stamp, 0).ok_or("clock.format: the timestamp is out of range")?;
    let local = utc.with_timezone(&chrono::Local);
    let items: Vec<Item> = StrftimeItems::new(pattern).collect();
    if items.iter().any(|it| matches!(it, Item::Error)) {
        return Err(format!("clock.format: \"{}\" is not a valid date pattern", pattern));
    }
    Ok(local.format_with_items(items.iter()).to_string())
}

/// Reads a local date-and-time written like `pattern` and returns its Unix
/// timestamp (seconds).
fn clock_parse(s: &str, pattern: &str) -> Result<i64, String> {
    use chrono::TimeZone;
    let naive = chrono::NaiveDateTime::parse_from_str(s, pattern)
        .map_err(|e| format!("clock.parse: {} does not match \"{}\" ({})", quote(s), pattern, e))?;
    match chrono::Local.from_local_datetime(&naive).single() {
        Some(dt) => Ok(dt.timestamp()),
        None => Err(format!("clock.parse: {} is an ambiguous or impossible local time", quote(s))),
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s)
}

// ---------------------------------------------------------------- running commands (the "shell" module)

/// Runs a command through the operating system's shell and returns its standard
/// output. A command that fails (non-zero exit) is a catchable run-time error
/// carrying its error output.
fn shell_run(command: &str) -> Result<String, String> {
    let mut c = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    let out = c.output().map_err(|e| format!("shell.run: cannot start the command ({})", e))?;
    if !out.status.success() {
        let code = out.status.code().map_or_else(|| "stopped by a signal".to_string(), |n| format!("exit code {}", n));
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("shell.run: the command failed ({}): {}", code, err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

// ---------------------------------------------------------------- helpers for the extra modules

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a as i64
}

fn is_prime(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    if n % 2 == 0 {
        return n == 2;
    }
    let mut d: i64 = 3;
    while d.saturating_mul(d) <= n {
        if n % d == 0 {
            return false;
        }
        d += 2;
    }
    true
}

fn prime_next(n: i64) -> i64 {
    let mut c = if n < 2 { 2 } else { n + 1 };
    while !is_prime(c) {
        c += 1;
    }
    c
}

fn prime_nth(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("prime.nth: the position starts at 1".to_string());
    }
    let (mut count, mut c) = (0, 1);
    while count < n {
        c += 1;
        if is_prime(c) {
            count += 1;
        }
    }
    Ok(c)
}

fn fib_at(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("fib.at: the position cannot be negative".to_string());
    }
    let (mut a, mut b): (i64, i64) = (0, 1);
    for _ in 0..n {
        let next = a.checked_add(b).ok_or(OVERFLOW)?;
        (a, b) = (b, next);
    }
    Ok(a)
}

fn int_to_base(n: i64, base: i64) -> Result<String, String> {
    if !(2..=36).contains(&base) {
        return Err("base.to: the base must be from 2 to 36".to_string());
    }
    if n == 0 {
        return Ok("0".to_string());
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let (base, mut v) = (base as u128, (n as i128).unsigned_abs());
    let mut out = Vec::new();
    while v > 0 {
        out.push(digits[(v % base) as usize]);
        v /= base;
    }
    if n < 0 {
        out.push(b'-');
    }
    out.reverse();
    Ok(String::from_utf8(out).unwrap())
}

fn base_from(s: &str, base: i64) -> Result<i64, String> {
    if !(2..=36).contains(&base) {
        return Err("base.from: the base must be from 2 to 36".to_string());
    }
    let t = s.trim();
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let n = i64::from_str_radix(body, base as u32)
        .map_err(|_| format!("base.from: \"{}\" is not a base-{} number", s, base))?;
    Ok(if neg { -n } else { n })
}

const ROMAN: &[(i64, &str)] = &[
    (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"),
    (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
];

fn roman_to(n: i64) -> Result<String, String> {
    if !(1..=3999).contains(&n) {
        return Err("roman.to: the number must be from 1 to 3999".to_string());
    }
    let mut n = n;
    let mut out = String::new();
    for &(v, sym) in ROMAN {
        while n >= v {
            out.push_str(sym);
            n -= v;
        }
    }
    Ok(out)
}

fn roman_from(s: &str) -> Result<i64, String> {
    let up = s.trim().to_uppercase();
    let val = |c: char| match c {
        'I' => 1, 'V' => 5, 'X' => 10, 'L' => 50, 'C' => 100, 'D' => 500, 'M' => 1000, _ => 0,
    };
    if up.is_empty() || up.chars().any(|c| val(c) == 0) {
        return Err(format!("roman.from: \"{}\" is not a Roman numeral", s));
    }
    let nums: Vec<i64> = up.chars().map(val).collect();
    let mut total = 0;
    for i in 0..nums.len() {
        if i + 1 < nums.len() && nums[i] < nums[i + 1] {
            total -= nums[i];
        } else {
            total += nums[i];
        }
    }
    // a valid numeral is the one roman_to would produce
    if roman_to(total).map(|r| r == up).unwrap_or(false) {
        Ok(total)
    } else {
        Err(format!("roman.from: \"{}\" is not a valid Roman numeral", s))
    }
}

fn size_human(n: i64) -> String {
    if n < 0 {
        return format!("-{}", size_human(-n));
    }
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    let (mut v, mut i) = (n as f64, 0);
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} B", n)
    } else {
        format!("{:.1} {}", v, units[i])
    }
}

fn duration_human(total: i64) -> String {
    if total < 0 {
        return format!("-{}", duration_human(-total));
    }
    if total == 0 {
        return "0s".to_string();
    }
    let parts = [(86400, "d"), (3600, "h"), (60, "m"), (1, "s")];
    let mut rest = total;
    let mut out = Vec::new();
    for (size, name) in parts {
        let n = rest / size;
        if n > 0 {
            out.push(format!("{}{}", n, name));
            rest %= size;
        }
    }
    out.join(" ")
}

fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // mean Earth radius, km
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dphi = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dphi / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn compass_direction(deg: f64) -> String {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let i = ((deg.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8;
    POINTS[i].to_string()
}

fn ordinal_of(n: i64) -> String {
    let last_two = n.unsigned_abs() % 100;
    let suffix = if (11..=13).contains(&last_two) {
        "th"
    } else {
        match n.unsigned_abs() % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{}{}", n, suffix)
}

fn pad(s: &str, width: i64, fill: &str, left: bool) -> String {
    let fc = fill.chars().next().unwrap_or(' ');
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let padding: String = std::iter::repeat(fc).take((width - len) as usize).collect();
    if left {
        format!("{}{}", padding, s)
    } else {
        format!("{}{}", s, padding)
    }
}

fn center(s: &str, width: i64, fill: &str) -> String {
    let fc = fill.chars().next().unwrap_or(' ');
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let total = (width - len) as usize;
    let left = total / 2;
    let fillup = |n: usize| std::iter::repeat(fc).take(n).collect::<String>();
    format!("{}{}{}", fillup(left), s, fillup(total - left))
}

fn hex_decode(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err("hex.decode: the text must have an even number of digits".to_string());
    }
    let b = s.as_bytes();
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for pair in b.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).ok_or("hex.decode: not a hex digit")?;
        let lo = (pair[1] as char).to_digit(16).ok_or("hex.decode: not a hex digit")?;
        bytes.push((hi * 16 + lo) as u8);
    }
    String::from_utf8(bytes).map_err(|_| "hex.decode: the data is not text".to_string())
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn url_decode(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                if i + 2 >= b.len() {
                    return Err("url.decode: a % is not followed by two hex digits".to_string());
                }
                let hi = (b[i + 1] as char).to_digit(16).ok_or("url.decode: bad hex after %")?;
                let lo = (b[i + 2] as char).to_digit(16).ok_or("url.decode: bad hex after %")?;
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "url.decode: the data is not text".to_string())
}

fn case_title(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Splits text into words, breaking on non-alphanumerics and at lowercase→uppercase.
fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && prev_lower && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
            prev_lower = c.is_lowercase() || c.is_numeric();
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn case_join(s: &str, sep: char) -> String {
    words(s).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join(&sep.to_string())
}

fn case_camel(s: &str) -> String {
    words(s)
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let low = w.to_lowercase();
            if i == 0 {
                low
            } else {
                let mut ch = low.chars();
                match ch.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect()
}

fn slug_make(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn all_chars(s: &str, pred: impl Fn(char) -> bool) -> bool {
    !s.is_empty() && s.chars().all(pred)
}

fn all_cased(s: &str, upper: bool) -> bool {
    let mut any = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            any = true;
            if upper != c.is_uppercase() {
                return false;
            }
        }
    }
    any
}

fn caesar(s: &str, shift: i64) -> String {
    let sh = shift.rem_euclid(26) as u8;
    s.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                (b'A' + (c as u8 - b'A' + sh) % 26) as char
            } else if c.is_ascii_lowercase() {
                (b'a' + (c as u8 - b'a' + sh) % 26) as char
            } else {
                c
            }
        })
        .collect()
}

fn leet_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c.to_ascii_lowercase() {
            'a' => '4',
            'e' => '3',
            'i' => '1',
            'o' => '0',
            's' => '5',
            't' => '7',
            _ => c,
        })
        .collect()
}

const MORSE: &[(char, &str)] = &[
    ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."), ('F', "..-."), ('G', "--."), ('H', "...."),
    ('I', ".."), ('J', ".---"), ('K', "-.-"), ('L', ".-.."), ('M', "--"), ('N', "-."), ('O', "---"), ('P', ".--."),
    ('Q', "--.-"), ('R', ".-."), ('S', "..."), ('T', "-"), ('U', "..-"), ('V', "...-"), ('W', ".--"), ('X', "-..-"),
    ('Y', "-.--"), ('Z', "--.."), ('0', "-----"), ('1', ".----"), ('2', "..---"), ('3', "...--"), ('4', "....-"),
    ('5', "....."), ('6', "-...."), ('7', "--..."), ('8', "---.."), ('9', "----."),
];

fn morse_encode(s: &str) -> Result<String, String> {
    let mut out = Vec::new();
    for c in s.to_uppercase().chars() {
        if c.is_whitespace() {
            out.push("/".to_string());
        } else if let Some((_, code)) = MORSE.iter().find(|(ch, _)| *ch == c) {
            out.push(code.to_string());
        } else {
            return Err(format!("morse.encode: cannot encode '{}'", c));
        }
    }
    Ok(out.join(" "))
}

fn morse_decode(s: &str) -> Result<String, String> {
    let mut out = String::new();
    for token in s.split_whitespace() {
        if token == "/" {
            out.push(' ');
        } else if let Some((ch, _)) = MORSE.iter().find(|(_, code)| *code == token) {
            out.push(*ch);
        } else {
            return Err(format!("morse.decode: unknown code \"{}\"", token));
        }
    }
    Ok(out)
}

const NATO: &[(char, &str)] = &[
    ('A', "Alpha"), ('B', "Bravo"), ('C', "Charlie"), ('D', "Delta"), ('E', "Echo"), ('F', "Foxtrot"), ('G', "Golf"),
    ('H', "Hotel"), ('I', "India"), ('J', "Juliett"), ('K', "Kilo"), ('L', "Lima"), ('M', "Mike"), ('N', "November"),
    ('O', "Oscar"), ('P', "Papa"), ('Q', "Quebec"), ('R', "Romeo"), ('S', "Sierra"), ('T', "Tango"), ('U', "Uniform"),
    ('V', "Victor"), ('W', "Whiskey"), ('X', "X-ray"), ('Y', "Yankee"), ('Z', "Zulu"), ('0', "Zero"), ('1', "One"),
    ('2', "Two"), ('3', "Three"), ('4', "Four"), ('5', "Five"), ('6', "Six"), ('7', "Seven"), ('8', "Eight"),
    ('9', "Nine"),
];

fn nato_encode(s: &str) -> String {
    s.to_uppercase()
        .chars()
        .filter_map(|c| NATO.iter().find(|(ch, _)| *ch == c).map(|(_, w)| (*w).to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn color_invert(s: &str) -> Result<String, String> {
    let t = s.trim();
    let h = t.strip_prefix('#').unwrap_or(t);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("color.invert: \"{}\" is not a #rrggbb color", s));
    }
    let n = u32::from_str_radix(h, 16).unwrap();
    Ok(format!("#{:06x}", 0xFF_FFFF - n))
}

fn regex_is(pattern: &str, s: &str) -> bool {
    regex::Regex::new(pattern).map(|re| re.is_match(s)).unwrap_or(false)
}

fn luhn_valid(s: &str) -> bool {
    let clean: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut sum = 0;
    for (i, c) in clean.chars().rev().enumerate() {
        let mut d = c.to_digit(10).unwrap();
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

fn isbn_valid(s: &str) -> bool {
    let clean: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    match clean.len() {
        10 => {
            let mut sum = 0i32;
            for (i, c) in clean.chars().enumerate() {
                let v = if i == 9 && (c == 'X' || c == 'x') {
                    10
                } else {
                    match c.to_digit(10) {
                        Some(d) => d as i32,
                        None => return false,
                    }
                };
                sum += (10 - i as i32) * v;
            }
            sum % 11 == 0
        }
        13 => {
            let mut sum = 0i32;
            for (i, c) in clean.chars().enumerate() {
                let d = match c.to_digit(10) {
                    Some(d) => d as i32,
                    None => return false,
                };
                sum += if i % 2 == 0 { d } else { 3 * d };
            }
            sum % 10 == 0
        }
        _ => false,
    }
}

enum PathPart {
    Name,
    Stem,
    Ext,
    Parent,
}

fn path_part(p: &str, part: PathPart) -> String {
    let path = std::path::Path::new(p);
    let piece = match part {
        PathPart::Name => path.file_name(),
        PathPart::Stem => path.file_stem(),
        PathPart::Ext => path.extension(),
        PathPart::Parent => return path.parent().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    piece.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The arguments the program was given, after the interpreter and the script path.
fn program_args() -> Vec<String> {
    std::env::args().skip(2).collect()
}

fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => out.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            }
        }
    }
    out.push(cur);
    out
}

fn csv_line(fields: impl Iterator<Item = String>) -> String {
    fields
        .map(|f| {
            if f.contains([',', '"', '\n']) {
                format!("\"{}\"", f.replace('"', "\"\""))
            } else {
                f
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The numbers of an array or list argument (for the `stats` module).
fn numbers(v: &Value) -> Result<Vec<f64>, String> {
    let items = seq(v).borrow();
    if items.is_empty() {
        return Err("the list is empty".to_string());
    }
    no_empty_elements(&items, "stats")?;
    Ok(items.iter().map(real).collect())
}

const OVERFLOW: &str = "number too large: the result exceeds the integer limits";

/// A float that must become an int (floor, ceiling, truncate, to_int).
fn to_int_checked(r: f64) -> Result<Value, String> {
    if !r.is_finite() || !(-9.2e18..=9.2e18).contains(&r) {
        return Err(OVERFLOW.to_string());
    }
    Ok(Value::Int(r as i64))
}

fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (x, y) => real(x).partial_cmp(&real(y)).unwrap_or(Ordering::Equal),
    }
}

fn same(a: &Value, b: &Value) -> bool {
    compare(a, b) == Ordering::Equal
}

/// Every element of a collection must have a value before it can be compared.
fn no_empty_elements(items: &[Value], name: &str) -> Result<(), String> {
    if items.iter().any(|v| matches!(v, Value::Null)) {
        return Err(format!("{}: the array has elements without a value", name));
    }
    Ok(())
}

/// Position (in characters, from 1) of the first `part` in `s`, 0 if absent.
fn find(s: &str, part: &str) -> i64 {
    s.find(part).map_or(0, |byte| s[..byte].chars().count() as i64 + 1)
}

fn degrees(r: f64) -> f64 {
    r.to_degrees()
}

/// Runs a standard library function; procedures return `Value::Null`.
pub fn call(b: Builtin, args: &[Value]) -> Result<Value, String> {
    use Builtin::*;
    Ok(match b {
        // conversions
        ToInt => match &args[0] {
            Value::Str(s) => {
                let t = s.trim();
                Value::Int(t.parse().map_err(|_| format!("to_int: \"{}\" is not a whole number", s))?)
            }
            Value::Real(r) => to_int_checked(r.trunc())?,
            Value::Int(n) => Value::Int(*n),
            Value::Bool(v) => Value::Int(*v as i64),
            other => panic!("internal error: to_int of {:?}", other),
        },
        ToFloat => match &args[0] {
            Value::Str(s) => match s.trim().replace(',', ".").parse::<f64>() {
                Ok(r) if r.is_finite() => Value::Real(r),
                _ => return Err(format!("to_float: \"{}\" is not a number", s)),
            },
            v => Value::Real(real(v)),
        },
        ToText => string(display(&args[0])),
        Char => {
            let n = int(&args[0]);
            let c = u32::try_from(n)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("char: {} is not a valid character code", n))?;
            string(c.to_string())
        }
        Code => match text(&args[0]).chars().next() {
            Some(c) => Value::Int(c as i64),
            None => return Err("code: the string is empty".to_string()),
        },
        ToHex | ToBinary => {
            let n = int(&args[0]);
            let digits = if b == ToHex { format!("{:x}", n.unsigned_abs()) } else { format!("{:b}", n.unsigned_abs()) };
            string(if n < 0 { format!("-{}", digits) } else { digits })
        }

        // math
        Absolute => match &args[0] {
            Value::Int(n) => Value::Int(n.checked_abs().ok_or(OVERFLOW)?),
            v => Value::Real(real(v).abs()),
        },
        Minimum | Maximum => {
            let pick = |a: Value, c: Value| {
                let take_c = match compare(&c, &a) {
                    Ordering::Less => b == Minimum,
                    Ordering::Greater => b == Maximum,
                    Ordering::Equal => false,
                };
                if take_c { c } else { a }
            };
            if args.len() == 2 {
                pick(args[0].clone(), args[1].clone())
            } else {
                let items = seq(&args[0]).borrow();
                no_empty_elements(&items, b.name())?;
                let mut it = items.iter().cloned();
                let first = it.next().ok_or_else(|| format!("{}: the list is empty", b.name()))?;
                it.fold(first, pick)
            }
        }
        Power => Value::Real(real(&args[0]).powf(real(&args[1]))),
        Floor => to_int_checked(real(&args[0]).floor())?,
        Ceiling => to_int_checked(real(&args[0]).ceil())?,
        Truncate => to_int_checked(real(&args[0]).trunc())?,
        RoundTo => {
            let digits = int(&args[1]).clamp(-15, 15) as i32;
            let scale = 10f64.powi(digits);
            Value::Real((real(&args[0]) * scale).round() / scale)
        }
        Exponential => Value::Real(real(&args[0]).exp()),
        Logarithm => {
            let x = real(&args[0]);
            if x <= 0.0 {
                return Err(format!("logarithm of a number that is not positive ({})", format_real(x)));
            }
            match args.get(1).map(real) {
                None => Value::Real(x.ln()),
                Some(base) if base > 0.0 && base != 1.0 => Value::Real(x.ln() / base.ln()),
                Some(base) => return Err(format!("logarithm: the base must be positive and not 1, here it is {}", format_real(base))),
            }
        }
        Tan => Value::Real(real(&args[0]).to_radians().tan()),
        Arcsin | Arccos => {
            let x = real(&args[0]);
            if !(-1.0..=1.0).contains(&x) {
                return Err(format!("{} works with numbers from -1 to 1, here it is {}", b.name(), format_real(x)));
            }
            Value::Real(degrees(if b == Arcsin { x.asin() } else { x.acos() }))
        }
        Arctan => Value::Real(degrees(real(&args[0]).atan())),
        Arctan2 => Value::Real(degrees(real(&args[0]).atan2(real(&args[1])))),
        Pi => Value::Real(std::f64::consts::PI),

        // strings
        Uppercase => string(text(&args[0]).to_uppercase()),
        Lowercase => string(text(&args[0]).to_lowercase()),
        Trim => string(text(&args[0]).trim()),
        Find => Value::Int(find(text(&args[0]), text(&args[1]))),
        Count => {
            let part = text(&args[1]);
            if part.is_empty() {
                return Err("count: the part to look for is empty".to_string());
            }
            Value::Int(text(&args[0]).matches(part).count() as i64)
        }
        Replace => {
            let old = text(&args[1]);
            if old.is_empty() {
                return Err("replace: the text to replace is empty".to_string());
            }
            string(text(&args[0]).replace(old, text(&args[2])))
        }
        Split => {
            let (s, sep) = (text(&args[0]), text(&args[1]));
            let pieces: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| string(c.to_string())).collect()
            } else {
                s.split(sep).map(string).collect()
            };
            list(pieces)
        }
        Join => {
            let items = seq(&args[0]).borrow();
            no_empty_elements(&items, "join")?;
            let parts: Vec<String> = items.iter().map(display).collect();
            string(parts.join(text(&args[1])))
        }
        StartsWith => Value::Bool(text(&args[0]).starts_with(text(&args[1]))),
        EndsWith => Value::Bool(text(&args[0]).ends_with(text(&args[1]))),
        Substring => {
            let chars: Vec<char> = text(&args[0]).chars().collect();
            let from = int(&args[1]).max(1);
            let to = int(&args[2]).min(chars.len() as i64);
            if from > to {
                string("")
            } else {
                string(chars[from as usize - 1..to as usize].iter().collect::<String>())
            }
        }
        Add => {
            set_data(&args[0]).borrow_mut().set(DictKey::of(&args[1]), Value::Bool(true));
            Value::Null
        }
        Discard => {
            set_data(&args[0]).borrow_mut().remove(&DictKey::of(&args[1]));
            Value::Null
        }
        ToList => new_list(set_data(&args[0]).borrow().keys().map(DictKey::value).collect()),
        Contains | IndexOf => match &args[0] {
            Value::Set(d) => Value::Bool(d.borrow().get(&DictKey::of(&args[1])).is_some()),
            Value::Str(s) => Value::Bool(s.contains(text(&args[1]))),
            c => {
                let items = seq(c).borrow();
                let at = items.iter().position(|v| !matches!(v, Value::Null) && same(v, &args[1]));
                if b == Contains {
                    Value::Bool(at.is_some())
                } else {
                    Value::Int(at.map_or(0, |i| i as i64 + 1))
                }
            }
        },

        // collections
        Sum => {
            let items = seq(&args[0]).borrow();
            no_empty_elements(&items, "sum")?;
            match items.first() {
                Some(Value::Real(_)) => Value::Real(items.iter().map(real).sum()),
                _ => {
                    let mut total: i64 = 0;
                    for v in items.iter() {
                        total = total.checked_add(int(v)).ok_or(OVERFLOW)?;
                    }
                    Value::Int(total)
                }
            }
        }
        Sort => {
            let mut items = seq(&args[0]).borrow_mut();
            no_empty_elements(&items, "sort")?;
            items.sort_by(compare);
            Value::Null
        }
        Reverse => {
            seq(&args[0]).borrow_mut().reverse();
            Value::Null
        }

        // dictionaries
        HasKey => Value::Bool(dict(&args[0]).borrow().get(&DictKey::of(&args[1])).is_some()),
        Get => dict(&args[0]).borrow().get(&DictKey::of(&args[1])).cloned().unwrap_or_else(|| args[2].clone()),
        RemoveKey => {
            let k = DictKey::of(&args[1]);
            if !dict(&args[0]).borrow_mut().remove(&k) {
                return Err(format!("remove_key: {}", missing_key(&k)));
            }
            Value::Null
        }
        Keys => new_list(dict(&args[0]).borrow().keys().map(DictKey::value).collect()),
        Values => new_list(dict(&args[0]).borrow().values().cloned().collect()),
        Insert => {
            let mut items = seq(&args[0]).borrow_mut();
            let pos = int(&args[1]);
            if pos < 1 || pos as usize > items.len() + 1 {
                return Err(format!("insert: the position must be from 1 to {}, here it is {}", items.len() + 1, pos));
            }
            items.insert(pos as usize - 1, args[2].clone());
            Value::Null
        }
        TakeLast => seq(&args[0]).borrow_mut().pop().ok_or("take_last: the list is empty")?,
        ClearList => {
            seq(&args[0]).borrow_mut().clear();
            Value::Null
        }

        // files
        ReadFile => {
            let path = text(&args[0]);
            string(std::fs::read_to_string(path).map_err(|e| format!("cannot read the file \"{}\": {}", path, e))?)
        }
        ReadLines => {
            let path = text(&args[0]);
            let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read the file \"{}\": {}", path, e))?;
            list(content.lines().map(string).collect())
        }
        WriteFile => {
            let path = text(&args[0]);
            std::fs::write(path, text(&args[1])).map_err(|e| format!("cannot write the file \"{}\": {}", path, e))?;
            Value::Null
        }
        AppendFile => {
            use std::io::Write;
            let path = text(&args[0]);
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| format!("cannot write the file \"{}\": {}", path, e))?;
            f.write_all(text(&args[1]).as_bytes()).map_err(|e| format!("cannot write the file \"{}\": {}", path, e))?;
            Value::Null
        }
        FileExists => Value::Bool(std::path::Path::new(text(&args[0])).exists()),

        // system
        Wait => {
            let ms = int(&args[0]).clamp(0, 86_400_000) as u64;
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Value::Null
        }
        Today => {
            let t = local_time();
            string(format!("{:04}-{:02}-{:02}", t.0, t.1, t.2))
        }
        Now => {
            let t = local_time();
            string(format!("{:02}:{:02}:{:02}", t.3, t.4, t.5))
        }
        TextMatches => Value::Bool(pattern(&args[1])?.is_match(text(&args[0]))),
        TextFind => string(pattern(&args[1])?.find(text(&args[0])).map_or("", |m| m.as_str())),
        TextReplaceAll => string(pattern(&args[1])?.replace_all(text(&args[0]), text(&args[2])).into_owned()),
        TextAll => list(pattern(&args[1])?.find_iter(text(&args[0])).map(|m| string(m.as_str())).collect()),
        HashSha256 => {
            use sha2::{Digest, Sha256};
            string(hex(Sha256::digest(text(&args[0]).as_bytes())))
        }
        HashSha512 => {
            use sha2::{Digest, Sha512};
            string(hex(Sha512::digest(text(&args[0]).as_bytes())))
        }
        WebGet => string(web_get(text(&args[0]))?),
        DataText => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))? {
                Json::String(s) => string(s.clone()),
                Json::Number(n) => string(n.to_string()),
                Json::Bool(b) => string(b.to_string()),
                _ => return Err(format!("the value at \"{}\" is not text", text(&args[1]))),
            }
        }
        DataNumber => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))?.as_f64() {
                Some(n) => Value::Real(n),
                None => return Err(format!("the value at \"{}\" is not a number", text(&args[1]))),
            }
        }
        DataFlag => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))?.as_bool() {
                Some(v) => Value::Bool(v),
                None => return Err(format!("the value at \"{}\" is not true or false", text(&args[1]))),
            }
        }
        DataHas => {
            let root = json_root(&args[0])?;
            Value::Bool(json_at(&root, text(&args[1])).is_ok())
        }
        DataSize => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))? {
                Json::Array(a) => Value::Int(a.len() as i64),
                Json::Object(o) => Value::Int(o.len() as i64),
                _ => return Err(format!("the value at \"{}\" is not a list or an object", text(&args[1]))),
            }
        }
        DataKeys => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))? {
                Json::Object(o) => list(o.keys().map(|k| string(k.clone())).collect()),
                _ => return Err(format!("the value at \"{}\" is not an object", text(&args[1]))),
            }
        }
        DataToJson => string(serde_json::to_string(&to_json_value(&args[0])?).map_err(|e| format!("cannot make JSON: {}", e))?),
        ClockStamp => Value::Int(chrono::Utc::now().timestamp()),
        ClockFormat => string(clock_format(int(&args[0]), text(&args[1]))?),
        ClockParse => Value::Int(clock_parse(text(&args[0]), text(&args[1]))?),
        EncodeBase64 => {
            use base64::{engine::general_purpose::STANDARD, Engine};
            string(STANDARD.encode(text(&args[0]).as_bytes()))
        }
        EncodeFromBase64 => {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let bytes = STANDARD
                .decode(text(&args[0]))
                .map_err(|e| format!("encode.from_base64: not valid base64 ({})", e))?;
            string(String::from_utf8(bytes).map_err(|_| "encode.from_base64: the decoded data is not text".to_string())?)
        }
        ShellRun => string(shell_run(text(&args[0]))?),

        // math
        MathGcd => Value::Int(gcd(int(&args[0]), int(&args[1]))),
        MathLcm => {
            let (x, y) = (int(&args[0]), int(&args[1]));
            let g = gcd(x, y);
            if g == 0 {
                Value::Int(0)
            } else {
                Value::Int((x / g).checked_mul(y).ok_or(OVERFLOW)?.abs())
            }
        }
        MathFactorial => {
            let n = int(&args[0]);
            if n < 0 {
                return Err("math.factorial: the number cannot be negative".to_string());
            }
            let mut acc: i64 = 1;
            for k in 2..=n {
                acc = acc.checked_mul(k).ok_or(OVERFLOW)?;
            }
            Value::Int(acc)
        }
        MathIsPrime => Value::Bool(is_prime(int(&args[0]))),
        MathSign => {
            let x = real(&args[0]);
            Value::Int(if x > 0.0 { 1 } else if x < 0.0 { -1 } else { 0 })
        }
        MathHypot => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        MathClamp => Value::Real(real(&args[0]).max(real(&args[1])).min(real(&args[2]))),
        MathCbrt => Value::Real(real(&args[0]).cbrt()),

        // bits
        BitsAnd => Value::Int(int(&args[0]) & int(&args[1])),
        BitsOr => Value::Int(int(&args[0]) | int(&args[1])),
        BitsXor => Value::Int(int(&args[0]) ^ int(&args[1])),
        BitsNot => Value::Int(!int(&args[0])),
        BitsLeft => Value::Int(int(&args[0]).wrapping_shl(int(&args[1]).clamp(0, 63) as u32)),
        BitsRight => Value::Int(int(&args[0]).wrapping_shr(int(&args[1]).clamp(0, 63) as u32)),
        BitsOnes => Value::Int(int(&args[0]).count_ones() as i64),

        // base and roman
        BaseTo => string(int_to_base(int(&args[0]), int(&args[1]))?),
        BaseFrom => Value::Int(base_from(text(&args[0]), int(&args[1]))?),
        RomanTo => string(roman_to(int(&args[0]))?),
        RomanFrom => Value::Int(roman_from(text(&args[0]))?),

        // temperatures, angles, percentages
        TempCToF => Value::Real(real(&args[0]) * 9.0 / 5.0 + 32.0),
        TempFToC => Value::Real((real(&args[0]) - 32.0) * 5.0 / 9.0),
        TempCToK => Value::Real(real(&args[0]) + 273.15),
        TempKToC => Value::Real(real(&args[0]) - 273.15),
        AngleToRadians => Value::Real(real(&args[0]).to_radians()),
        AngleToDegrees => Value::Real(real(&args[0]).to_degrees()),
        AngleNormalize => Value::Real(real(&args[0]).rem_euclid(360.0)),
        PercentOf => Value::Real(real(&args[0]) / 100.0 * real(&args[1])),
        PercentChange => {
            let (from, to) = (real(&args[0]), real(&args[1]));
            if from == 0.0 {
                return Err("percent.change: the starting value is zero".to_string());
            }
            Value::Real((to - from) / from * 100.0)
        }

        // geometry and units
        GeomCircleArea => Value::Real(std::f64::consts::PI * real(&args[0]).powi(2)),
        GeomRectArea => Value::Real(real(&args[0]) * real(&args[1])),
        GeomTriangleArea => Value::Real(real(&args[0]) * real(&args[1]) / 2.0),
        GeomSphereVolume => Value::Real(4.0 / 3.0 * std::f64::consts::PI * real(&args[0]).powi(3)),
        GeomCubeVolume => Value::Real(real(&args[0]).powi(3)),
        LengthMToFt => Value::Real(real(&args[0]) * 3.280_839_895),
        LengthFtToM => Value::Real(real(&args[0]) / 3.280_839_895),
        LengthKmToMi => Value::Real(real(&args[0]) * 0.621_371_192),
        LengthMiToKm => Value::Real(real(&args[0]) / 0.621_371_192),
        WeightKgToLb => Value::Real(real(&args[0]) * 2.204_622_622),
        WeightLbToKg => Value::Real(real(&args[0]) / 2.204_622_622),
        SizeHuman => string(size_human(int(&args[0]))),
        DurationHuman => string(duration_human(int(&args[0]))),
        GeoDistance => Value::Real(haversine(real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]))),
        TaxAdd => Value::Real(real(&args[0]) * (1.0 + real(&args[1]) / 100.0)),
        TaxRemove => Value::Real(real(&args[0]) / (1.0 + real(&args[1]) / 100.0)),
        CompassDirection => string(compass_direction(real(&args[0]))),

        // number helpers
        OrdinalOf => string(ordinal_of(int(&args[0]))),
        FibAt => Value::Int(fib_at(int(&args[0]))?),
        PrimeNext => Value::Int(prime_next(int(&args[0]))),
        PrimeNth => Value::Int(prime_nth(int(&args[0]))?),

        // strings
        StrRepeat => {
            let (s, n) = (text(&args[0]), int(&args[1]));
            if n < 0 {
                return Err("str.repeat: the count cannot be negative".to_string());
            }
            if s.len().saturating_mul(n as usize) > 50_000_000 {
                return Err("str.repeat: the result would be too large".to_string());
            }
            string(s.repeat(n as usize))
        }
        StrReverse => string(text(&args[0]).chars().rev().collect::<String>()),
        StrPadLeft => string(pad(text(&args[0]), int(&args[1]), text(&args[2]), true)),
        StrPadRight => string(pad(text(&args[0]), int(&args[1]), text(&args[2]), false)),
        StrCenter => string(center(text(&args[0]), int(&args[1]), text(&args[2]))),
        HexEncode => string(hex(text(&args[0]).as_bytes())),
        HexDecode => string(hex_decode(text(&args[0]))?),
        UrlEncode => string(url_encode(text(&args[0]))),
        UrlDecode => string(url_decode(text(&args[0]))?),
        CaseTitle => string(case_title(text(&args[0]))),
        CaseSnake => string(case_join(text(&args[0]), '_')),
        CaseKebab => string(case_join(text(&args[0]), '-')),
        CaseCamel => string(case_camel(text(&args[0]))),
        SlugMake => string(slug_make(text(&args[0]))),
        AsciiIsDigit => Value::Bool(all_chars(text(&args[0]), |c| c.is_ascii_digit())),
        AsciiIsAlpha => Value::Bool(all_chars(text(&args[0]), |c| c.is_ascii_alphabetic())),
        AsciiIsSpace => Value::Bool(all_chars(text(&args[0]), |c| c.is_ascii_whitespace())),
        AsciiIsUpper => Value::Bool(all_cased(text(&args[0]), true)),
        AsciiIsLower => Value::Bool(all_cased(text(&args[0]), false)),
        MorseEncode => string(morse_encode(text(&args[0]))?),
        MorseDecode => string(morse_decode(text(&args[0]))?),
        CaesarEncode => string(caesar(text(&args[0]), int(&args[1]))),
        CaesarDecode => string(caesar(text(&args[0]), -int(&args[1]))),
        Rot13Apply => string(caesar(text(&args[0]), 13)),
        LeetEncode => string(leet_encode(text(&args[0]))),
        NatoEncode => string(nato_encode(text(&args[0]))),

        // colors and validation
        ColorHex => {
            let c = |v: &Value| int(v).clamp(0, 255);
            string(format!("#{:02x}{:02x}{:02x}", c(&args[0]), c(&args[1]), c(&args[2])))
        }
        ColorGray => {
            let v = int(&args[0]).clamp(0, 255);
            string(format!("#{:02x}{:02x}{:02x}", v, v, v))
        }
        ColorInvert => string(color_invert(text(&args[0]))?),
        ValidEmail => Value::Bool(regex_is(r"^[^@\s]+@[^@\s]+\.[^@\s]+$", text(&args[0]))),
        ValidInteger => Value::Bool(regex_is(r"^[+-]?[0-9]+$", text(&args[0]).trim())),
        ValidNumber => Value::Bool(regex_is(r"^[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)$", text(&args[0]).trim())),
        LuhnValid => Value::Bool(luhn_valid(text(&args[0]))),
        IsbnValid => Value::Bool(isbn_valid(text(&args[0]))),
        UuidMake => string(uuid::Uuid::new_v4().to_string()),
        CrcValue => Value::Int(i64::from(crc32fast::hash(text(&args[0]).as_bytes()))),

        // environment, paths and the operating system
        EnvGet => string(std::env::var(text(&args[0])).unwrap_or_default()),
        EnvHas => Value::Bool(std::env::var_os(text(&args[0])).is_some()),
        EnvSet => {
            std::env::set_var(text(&args[0]), text(&args[1]));
            Value::Null
        }
        PathJoin => string(std::path::Path::new(text(&args[0])).join(text(&args[1])).to_string_lossy().into_owned()),
        PathName => string(path_part(text(&args[0]), PathPart::Name)),
        PathStem => string(path_part(text(&args[0]), PathPart::Stem)),
        PathExtension => string(path_part(text(&args[0]), PathPart::Ext)),
        PathParent => string(path_part(text(&args[0]), PathPart::Parent)),
        PathIsAbsolute => Value::Bool(std::path::Path::new(text(&args[0])).is_absolute()),
        OsName => string(std::env::consts::OS),
        OsArch => string(std::env::consts::ARCH),
        OsFamily => string(std::env::consts::FAMILY),
        DirList => {
            let path = text(&args[0]);
            let mut names = Vec::new();
            for entry in std::fs::read_dir(path).map_err(|e| format!("dir.list: cannot read \"{}\": {}", path, e))? {
                let entry = entry.map_err(|e| format!("dir.list: {}", e))?;
                names.push(string(entry.file_name().to_string_lossy().into_owned()));
            }
            list(names)
        }
        DirMake => {
            let p = text(&args[0]);
            std::fs::create_dir_all(p).map_err(|e| format!("dir.make: cannot create \"{}\": {}", p, e))?;
            Value::Null
        }
        DirCurrent => string(std::env::current_dir().map_err(|e| format!("dir.current: {}", e))?.to_string_lossy().into_owned()),
        DirExists => Value::Bool(std::path::Path::new(text(&args[0])).is_dir()),
        FileSize => {
            let p = text(&args[0]);
            let m = std::fs::metadata(p).map_err(|e| format!("file.size: cannot read \"{}\": {}", p, e))?;
            Value::Int(m.len() as i64)
        }
        FileCopy => {
            std::fs::copy(text(&args[0]), text(&args[1])).map_err(|e| format!("file.copy: {}", e))?;
            Value::Null
        }
        FileRename => {
            std::fs::rename(text(&args[0]), text(&args[1])).map_err(|e| format!("file.rename: {}", e))?;
            Value::Null
        }
        FileModified => {
            let p = text(&args[0]);
            let m = std::fs::metadata(p).map_err(|e| format!("file.modified: {}", e))?;
            let t = m.modified().map_err(|e| format!("file.modified: {}", e))?;
            Value::Int(t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0))
        }
        FileDelete => {
            let p = text(&args[0]);
            std::fs::remove_file(p).map_err(|e| format!("file.delete: cannot delete \"{}\": {}", p, e))?;
            Value::Null
        }
        TmpPath => string(std::env::temp_dir().to_string_lossy().into_owned()),
        ArgsCount => Value::Int(program_args().len() as i64),
        ArgsGet => {
            let a = program_args();
            let i = int(&args[0]);
            if i < 1 || i as usize > a.len() {
                return Err(format!("args.get: there is no argument number {}", i));
            }
            string(a[i as usize - 1].clone())
        }

        // CSV and statistics
        CsvFields => list(csv_fields(text(&args[0])).into_iter().map(string).collect()),
        CsvLine => {
            let items = seq(&args[0]).borrow();
            string(csv_line(items.iter().map(display)))
        }
        StatsMean => {
            let v = numbers(&args[0])?;
            Value::Real(v.iter().sum::<f64>() / v.len() as f64)
        }
        StatsMedian => {
            let mut v = numbers(&args[0])?;
            v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            let n = v.len();
            Value::Real(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
        }
        StatsStdev => {
            let v = numbers(&args[0])?;
            let m = v.iter().sum::<f64>() / v.len() as f64;
            let var = v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64;
            Value::Real(var.sqrt())
        }
        other => panic!("internal error: {:?} is not a library function", other),
    })
}

/// (year, month, day, hour, minute, second) in local time.
#[cfg(windows)]
fn local_time() -> (u16, u16, u16, u16, u16, u16) {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(t: *mut SystemTime);
    }
    let mut t = SystemTime { year: 0, month: 0, day_of_week: 0, day: 0, hour: 0, minute: 0, second: 0, milliseconds: 0 };
    // SAFETY: GetLocalTime fills the struct it is given and keeps no pointer.
    unsafe { GetLocalTime(&mut t) };
    (t.year, t.month, t.day, t.hour, t.minute, t.second)
}

/// (year, month, day, hour, minute, second) in UTC.
#[cfg(not(windows))]
fn local_time() -> (u16, u16, u16, u16, u16, u16) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year as u16, month as u16, day as u16, (rem / 3600) as u16, (rem % 3600 / 60) as u16, (rem % 60) as u16)
}
