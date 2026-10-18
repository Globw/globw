//! The standard library: conversions, math, strings, collections, files and
//! system functions, with Python-like behavior and English names.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use rayon::prelude::*;
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
        GridOf => {
            arity(1)?;
            match collection(a[0]) {
                Some(e) if numeric(e) => ok(none(1), Type::Base(Int)),
                _ => other("grid.of works with an array or list of numbers"),
            }
        }
        GridToList => {
            arity(1)?;
            if a[0] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Int) });
            }
            ok(none(1), Type::List(Some(Real)))
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

fn permutations(n: i64, k: i64) -> Result<i64, String> {
    if n < 0 || k < 0 {
        return Err("choose: the values cannot be negative".to_string());
    }
    if k > n {
        return Ok(0);
    }
    let mut acc: i64 = 1;
    for i in 0..k {
        acc = acc.checked_mul(n - i).ok_or(OVERFLOW)?;
    }
    Ok(acc)
}

fn combinations(n: i64, k: i64) -> Result<i64, String> {
    if n < 0 || k < 0 {
        return Err("choose: the values cannot be negative".to_string());
    }
    if k > n {
        return Ok(0);
    }
    let k = k.min(n - k);
    let mut acc: i64 = 1;
    for i in 0..k {
        acc = acc.checked_mul(n - i).ok_or(OVERFLOW)? / (i + 1);
    }
    Ok(acc)
}

fn catalan(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("seq.catalan: the position cannot be negative".to_string());
    }
    Ok(combinations(2 * n, n)? / (n + 1))
}

fn mod_pow(base: i64, exp: i64, m: i64) -> Result<i64, String> {
    if m <= 0 {
        return Err("modular.pow: the modulus must be positive".to_string());
    }
    if exp < 0 {
        return Err("modular.pow: the exponent cannot be negative".to_string());
    }
    let m = m as i128;
    let mut result: i128 = 1 % m;
    let mut b = (base as i128).rem_euclid(m);
    let mut e = exp as u64;
    while e > 0 {
        if e & 1 == 1 {
            result = result * b % m;
        }
        b = b * b % m;
        e >>= 1;
    }
    Ok(result as i64)
}

fn collatz(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("collatz.steps: the number must be positive".to_string());
    }
    let mut n = n as i128;
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
        if steps > 10_000_000 {
            return Err("collatz.steps: too many steps".to_string());
        }
    }
    Ok(steps)
}

fn is_perfect(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    let mut sum = 1i64;
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            sum += d;
            if d != n / d {
                sum += n / d;
            }
        }
        d += 1;
    }
    sum == n
}

fn is_armstrong(n: i64) -> bool {
    if n < 0 {
        return false;
    }
    let s = n.to_string();
    let power = s.len() as u32;
    let sum: u128 = s.bytes().map(|b| ((b - b'0') as u128).pow(power)).sum();
    sum == n as u128
}

fn mask_middle(s: &str, keep: i64) -> String {
    let chars: Vec<char> = s.chars().collect();
    let k = keep.max(0) as usize;
    if chars.len() <= 2 * k {
        return "*".repeat(chars.len());
    }
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| if i < k || i >= chars.len() - k { *c } else { '*' })
        .collect()
}

fn pig_latin(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let lower = w.to_lowercase();
            let chars: Vec<char> = lower.chars().collect();
            if chars.is_empty() {
                return String::new();
            }
            if "aeiou".contains(chars[0]) {
                format!("{}way", lower)
            } else {
                let idx = chars.iter().position(|c| "aeiou".contains(*c)).unwrap_or(chars.len());
                let head: String = chars[..idx].iter().collect();
                let tail: String = chars[idx..].iter().collect();
                format!("{}{}ay", tail, head)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn caesar_char(c: char, shift: i64) -> char {
    let sh = shift.rem_euclid(26) as u8;
    if c.is_ascii_uppercase() {
        (b'A' + (c as u8 - b'A' + sh) % 26) as char
    } else if c.is_ascii_lowercase() {
        (b'a' + (c as u8 - b'a' + sh) % 26) as char
    } else {
        c
    }
}

fn atbash(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                (b'z' - (c as u8 - b'a')) as char
            } else if c.is_ascii_uppercase() {
                (b'Z' - (c as u8 - b'A')) as char
            } else {
                c
            }
        })
        .collect()
}

fn vigenere(s: &str, key: &str, encode: bool) -> Result<String, String> {
    let k: Vec<i64> = key.chars().filter(|c| c.is_ascii_alphabetic()).map(|c| (c.to_ascii_lowercase() as u8 - b'a') as i64).collect();
    if k.is_empty() {
        return Err("vigenere: the key must contain letters".to_string());
    }
    let mut ki = 0;
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            let shift = if encode { k[ki % k.len()] } else { -k[ki % k.len()] };
            out.push(caesar_char(c, shift));
            ki += 1;
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

fn rot47(s: &str) -> String {
    s.chars()
        .map(|c| {
            let b = c as u32;
            if (33..=126).contains(&b) {
                char::from_u32(33 + (b - 33 + 47) % 94).unwrap()
            } else {
                c
            }
        })
        .collect()
}

fn bin_decode(s: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    for group in s.split_whitespace() {
        bytes.push(u8::from_str_radix(group, 2).map_err(|_| format!("bin.decode: \"{}\" is not 8 bits", group))?);
    }
    String::from_utf8(bytes).map_err(|_| "bin.decode: the data is not text".to_string())
}

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(5) {
        let mut buf = [0u8; 5];
        buf[..chunk.len()].copy_from_slice(chunk);
        let n = (u64::from(buf[0]) << 32)
            | (u64::from(buf[1]) << 24)
            | (u64::from(buf[2]) << 16)
            | (u64::from(buf[3]) << 8)
            | u64::from(buf[4]);
        let keep = match chunk.len() {
            1 => 2,
            2 => 4,
            3 => 5,
            4 => 7,
            _ => 8,
        };
        for i in 0..8 {
            if i < keep {
                out.push(B32[((n >> (35 - i * 5)) & 31) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base32_decode(s: &str) -> Result<String, String> {
    let mut bits = 0u64;
    let mut nbits = 0u32;
    let mut out = Vec::new();
    for ch in s.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=') {
        let up = ch.to_ascii_uppercase();
        let val = B32.iter().position(|&x| x == up).ok_or_else(|| format!("base32.decode: '{}' is not a base32 character", ch as char))? as u64;
        bits = (bits << 5) | val;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    String::from_utf8(out).map_err(|_| "base32.decode: the data is not text".to_string())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn html_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

fn anagram(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let mut v: Vec<char> = s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
        v.sort_unstable();
        v
    };
    !a.is_empty() && norm(a) == norm(b)
}

fn rle_encode(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let mut count = 1;
        while chars.peek() == Some(&c) {
            chars.next();
            count += 1;
        }
        out.push(c);
        out.push_str(&count.to_string());
    }
    out
}

fn rle_decode(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let mut num = String::new();
        while let Some(d) = chars.peek() {
            if d.is_ascii_digit() {
                num.push(*d);
                chars.next();
            } else {
                break;
            }
        }
        if num.is_empty() {
            return Err("rle.decode: a character is not followed by a count".to_string());
        }
        let count: usize = num.parse().map_err(|_| "rle.decode: the count is too large".to_string())?;
        if count > 10_000_000 {
            return Err("rle.decode: the count is too large".to_string());
        }
        for _ in 0..count {
            out.push(c);
        }
    }
    Ok(out)
}

fn wrap_at(s: &str, width: i64) -> String {
    let w = width.max(1) as usize;
    let mut lines = Vec::new();
    for para in s.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if line.is_empty() {
                line = word.to_string();
            } else if line.chars().count() + 1 + word.chars().count() <= w {
                line.push(' ');
                line.push_str(word);
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn spongebob(s: &str) -> String {
    let mut upper = false;
    s.chars()
        .map(|c| {
            if c.is_alphabetic() {
                upper = !upper;
                if upper {
                    c.to_ascii_uppercase()
                } else {
                    c.to_ascii_lowercase()
                }
            } else {
                c
            }
        })
        .collect()
}

fn boolv(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => panic!("internal error: expected a bool, found {:?}", other),
    }
}

fn bmi_category(b: f64) -> String {
    (if b < 18.5 {
        "underweight"
    } else if b < 25.0 {
        "normal"
    } else if b < 30.0 {
        "overweight"
    } else {
        "obese"
    })
    .to_string()
}

fn planet_gravity(name: &str) -> Result<f64, String> {
    Ok(match name.to_lowercase().as_str() {
        "mercury" => 3.7,
        "venus" => 8.87,
        "earth" => 9.81,
        "moon" => 1.62,
        "mars" => 3.71,
        "jupiter" => 24.79,
        "saturn" => 10.44,
        "uranus" => 8.69,
        "neptune" => 11.15,
        "pluto" => 0.62,
        "sun" => 274.0,
        _ => return Err(format!("gravity.on: unknown place \"{}\"", name)),
    })
}

fn loan_payment(principal: f64, annual_rate: f64, months: i64) -> Result<f64, String> {
    if months <= 0 {
        return Err("loan.payment: the number of months must be positive".to_string());
    }
    let r = annual_rate / 100.0 / 12.0;
    let n = months as f64;
    if r == 0.0 {
        return Ok(principal / n);
    }
    Ok(principal * r * (1.0 + r).powf(n) / ((1.0 + r).powf(n) - 1.0))
}

fn money_format(amount: f64) -> String {
    let neg = amount < 0.0;
    let s = format!("{:.2}", amount.abs());
    let (int_part, dec) = s.split_once('.').unwrap_or((s.as_str(), "00"));
    let bytes = int_part.as_bytes();
    let mut grouped = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*b as char);
    }
    format!("{}{}.{}", if neg { "-" } else { "" }, grouped, dec)
}

fn bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

fn heron(a: f64, b: f64, c: f64) -> Result<f64, String> {
    if a <= 0.0 || b <= 0.0 || c <= 0.0 || a + b <= c || a + c <= b || b + c <= a {
        return Err("triangle.heron: these sides do not form a triangle".to_string());
    }
    let s = (a + b + c) / 2.0;
    Ok((s * (s - a) * (s - b) * (s - c)).sqrt())
}

fn weekday_of(stamp: i64) -> Result<String, String> {
    use chrono::Datelike;
    let dt = chrono::DateTime::from_timestamp(stamp, 0).ok_or("weekday.of: the timestamp is out of range")?;
    const NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    Ok(NAMES[dt.weekday().num_days_from_monday() as usize].to_string())
}

fn month_name(n: i64) -> Result<String, String> {
    const M: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
        "December",
    ];
    if !(1..=12).contains(&n) {
        return Err(format!("month.name: {} is not a month (1-12)", n));
    }
    Ok(M[(n - 1) as usize].to_string())
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> Result<i64, String> {
    Ok(match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => return Err(format!("month.days: {} is not a month (1-12)", m)),
    })
}

fn age_years(birth: i64, now: i64) -> i64 {
    use chrono::Datelike;
    match (chrono::DateTime::from_timestamp(birth, 0), chrono::DateTime::from_timestamp(now, 0)) {
        (Some(b), Some(n)) => {
            let mut years = n.year() - b.year();
            if (n.month(), n.day()) < (b.month(), b.day()) {
                years -= 1;
            }
            years as i64
        }
        _ => 0,
    }
}

fn zodiac_sign(month: i64, day: i64) -> Result<String, String> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err("zodiac.sign: that is not a valid date".to_string());
    }
    let sign = match month {
        1 => if day < 20 { "Capricorn" } else { "Aquarius" },
        2 => if day < 19 { "Aquarius" } else { "Pisces" },
        3 => if day < 21 { "Pisces" } else { "Aries" },
        4 => if day < 20 { "Aries" } else { "Taurus" },
        5 => if day < 21 { "Taurus" } else { "Gemini" },
        6 => if day < 21 { "Gemini" } else { "Cancer" },
        7 => if day < 23 { "Cancer" } else { "Leo" },
        8 => if day < 23 { "Leo" } else { "Virgo" },
        9 => if day < 23 { "Virgo" } else { "Libra" },
        10 => if day < 23 { "Libra" } else { "Scorpio" },
        11 => if day < 22 { "Scorpio" } else { "Sagittarius" },
        _ => if day < 22 { "Sagittarius" } else { "Capricorn" },
    };
    Ok(sign.to_string())
}

fn season_of(m: i64) -> Result<String, String> {
    Ok(match m {
        12 | 1 | 2 => "winter",
        3 | 4 | 5 => "spring",
        6 | 7 | 8 => "summer",
        9 | 10 | 11 => "autumn",
        _ => return Err(format!("season.of: {} is not a month", m)),
    }
    .to_string())
}

fn grade_letter(score: f64) -> String {
    (if score >= 90.0 {
        "A"
    } else if score >= 80.0 {
        "B"
    } else if score >= 70.0 {
        "C"
    } else if score >= 60.0 {
        "D"
    } else {
        "F"
    })
    .to_string()
}

fn fraction_simplify(n: i64, d: i64) -> Result<String, String> {
    if d == 0 {
        return Err("fraction.simplify: the denominator is zero".to_string());
    }
    let g = gcd(n, d).max(1);
    let (mut nn, mut dd) = (n / g, d / g);
    if dd < 0 {
        nn = -nn;
        dd = -dd;
    }
    Ok(format!("{}/{}", nn, dd))
}

fn fnv1a(s: &str) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h as i64
}

fn djb2(s: &str) -> i64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h as i64
}

fn sdbm(s: &str) -> i64 {
    let mut h: u64 = 0;
    for b in s.bytes() {
        h = (b as u64).wrapping_add(h.wrapping_shl(6)).wrapping_add(h.wrapping_shl(16)).wrapping_sub(h);
    }
    h as i64
}

fn adler32(s: &str) -> i64 {
    let (mut a, mut b): (u32, u32) = (1, 0);
    for byte in s.bytes() {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (((b << 16) | a) as u64) as i64
}

fn ipv4_valid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()) && p.parse::<u32>().map(|n| n <= 255).unwrap_or(false)
        })
}

thread_local! {
    static RNG: std::cell::Cell<u64> = std::cell::Cell::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64)
            | 1,
    );
}

fn rng_u64() -> u64 {
    RNG.with(|c| {
        let mut x = c.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        c.set(x);
        x
    })
}

fn rng_range(lo: i64, hi: i64) -> i64 {
    if lo >= hi {
        return lo;
    }
    let span = (hi as i128 - lo as i128 + 1) as u128;
    (lo as i128 + (rng_u64() as u128 % span) as i128) as i64
}

fn rng_hex(len: i64) -> Result<String, String> {
    if len < 0 {
        return Err("token.hex: the length cannot be negative".to_string());
    }
    if len > 100_000 {
        return Err("token.hex: the length is too large".to_string());
    }
    let digits = b"0123456789abcdef";
    Ok((0..len).map(|_| digits[(rng_u64() % 16) as usize] as char).collect())
}

fn rng_password(len: i64) -> Result<String, String> {
    if len < 0 {
        return Err("password.make: the length cannot be negative".to_string());
    }
    if len > 100_000 {
        return Err("password.make: the length is too large".to_string());
    }
    let set = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%&*?";
    Ok((0..len).map(|_| set[(rng_u64() % set.len() as u64) as usize] as char).collect())
}

fn rng_nanoid() -> String {
    let set = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    (0..21).map(|_| set[(rng_u64() % 64) as usize] as char).collect()
}

const LOREM: &[&str] = &[
    "lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do", "eiusmod", "tempor",
    "incididunt", "ut", "labore", "et", "dolore", "magna", "aliqua",
];

fn lorem_words(n: i64) -> String {
    let n = n.max(0) as usize;
    (0..n).map(|i| LOREM[i % LOREM.len()]).collect::<Vec<_>>().join(" ")
}

fn caps_first(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

fn divisor_count(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("factor.count: the number must be positive".to_string());
    }
    let mut count = 0;
    let mut d = 1;
    while d * d <= n {
        if n % d == 0 {
            count += if d == n / d { 1 } else { 2 };
        }
        d += 1;
    }
    Ok(count)
}

fn divisor_sum(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("factor.sum: the number must be positive".to_string());
    }
    let mut sum = 0i64;
    let mut d = 1;
    while d * d <= n {
        if n % d == 0 {
            sum += d;
            if d != n / d {
                sum += n / d;
            }
        }
        d += 1;
    }
    Ok(sum)
}

fn totient(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("totient.of: the number must be positive".to_string());
    }
    let mut result = n;
    let mut nn = n;
    let mut p = 2;
    while p * p <= nn {
        if nn % p == 0 {
            while nn % p == 0 {
                nn /= p;
            }
            result -= result / p;
        }
        p += 1;
    }
    if nn > 1 {
        result -= result / nn;
    }
    Ok(result)
}

fn heat_index(tc: f64, rh: f64) -> f64 {
    let t = tc * 9.0 / 5.0 + 32.0;
    let hi = -42.379 + 2.049_015_23 * t + 10.143_331_27 * rh - 0.224_755_41 * t * rh
        - 0.006_837_83 * t * t
        - 0.054_817_17 * rh * rh
        + 0.001_228_74 * t * t * rh
        + 0.000_852_82 * t * rh * rh
        - 0.000_001_99 * t * t * rh * rh;
    (hi - 32.0) * 5.0 / 9.0
}

fn quadratic_root(a: f64, b: f64, c: f64, plus: bool) -> Result<f64, String> {
    if a == 0.0 {
        return Err("quadratic: 'a' is zero, so this is not a quadratic equation".to_string());
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return Err("quadratic: there are no real roots".to_string());
    }
    let sq = disc.sqrt();
    Ok(if plus { (-b + sq) / (2.0 * a) } else { (-b - sq) / (2.0 * a) })
}

// ---------------------------------------------------------------- the "grid" array engine
//
// Grids are contiguous Vec<f64> buffers kept in a registry; a Globw program holds
// an int handle. Element-wise kernels and reductions loop over unboxed memory in
// parallel (rayon), which is where the speed against a scalar loop comes from.

thread_local! {
    static GRIDS: RefCell<Vec<Vec<f64>>> = const { RefCell::new(Vec::new()) };
}

fn grid_new(data: Vec<f64>) -> i64 {
    GRIDS.with(|g| {
        let mut g = g.borrow_mut();
        g.push(data);
        (g.len() - 1) as i64
    })
}

fn grid_read<T>(h: i64, f: impl FnOnce(&[f64]) -> T) -> Result<T, String> {
    GRIDS.with(|g| {
        let g = g.borrow();
        let v = usize::try_from(h).ok().and_then(|i| g.get(i)).ok_or("grid: invalid handle")?;
        Ok(f(v))
    })
}

fn grid_map(h: i64, f: impl Fn(f64) -> f64 + Sync + Send) -> Result<i64, String> {
    let out: Vec<f64> = grid_read(h, |v| v.par_iter().map(|&x| f(x)).collect())?;
    Ok(grid_new(out))
}

fn grid_zip(a: i64, b: i64, f: impl Fn(f64, f64) -> f64 + Sync + Send) -> Result<i64, String> {
    let out = GRIDS.with(|g| {
        let g = g.borrow();
        let va = usize::try_from(a).ok().and_then(|i| g.get(i)).ok_or("grid: invalid handle")?;
        let vb = usize::try_from(b).ok().and_then(|i| g.get(i)).ok_or("grid: invalid handle")?;
        if va.len() != vb.len() {
            return Err(format!("grid: the sizes differ ({} and {})", va.len(), vb.len()));
        }
        Ok::<Vec<f64>, String>(va.par_iter().zip(vb.par_iter()).map(|(&x, &y)| f(x, y)).collect())
    })?;
    Ok(grid_new(out))
}

fn grid_dot(a: i64, b: i64) -> Result<Value, String> {
    GRIDS.with(|g| {
        let g = g.borrow();
        let va = usize::try_from(a).ok().and_then(|i| g.get(i)).ok_or("grid: invalid handle")?;
        let vb = usize::try_from(b).ok().and_then(|i| g.get(i)).ok_or("grid: invalid handle")?;
        if va.len() != vb.len() {
            return Err(format!("grid.dot: the sizes differ ({} and {})", va.len(), vb.len()));
        }
        Ok(Value::Real(va.par_iter().zip(vb.par_iter()).map(|(&x, &y)| x * y).sum()))
    })
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
        // batch A: unit conversions
        AreaSqmToSqft => Value::Real(real(&args[0]) * 10.763_910_4),
        AreaSqftToSqm => Value::Real(real(&args[0]) / 10.763_910_4),
        AreaAcreToSqm => Value::Real(real(&args[0]) * 4046.856_42),
        AreaSqmToAcre => Value::Real(real(&args[0]) / 4046.856_42),
        AreaHectareToAcre => Value::Real(real(&args[0]) * 2.471_053_8),
        AreaAcreToHectare => Value::Real(real(&args[0]) / 2.471_053_8),
        VolLToGal => Value::Real(real(&args[0]) * 0.264_172_05),
        VolGalToL => Value::Real(real(&args[0]) / 0.264_172_05),
        VolLToMl => Value::Real(real(&args[0]) * 1000.0),
        VolMlToL => Value::Real(real(&args[0]) / 1000.0),
        VolCupToMl => Value::Real(real(&args[0]) * 236.588_24),
        VolMlToCup => Value::Real(real(&args[0]) / 236.588_24),
        SpeedKmhToMph => Value::Real(real(&args[0]) * 0.621_371_19),
        SpeedMphToKmh => Value::Real(real(&args[0]) / 0.621_371_19),
        SpeedMpsToKmh => Value::Real(real(&args[0]) * 3.6),
        SpeedKmhToMps => Value::Real(real(&args[0]) / 3.6),
        SpeedKnotToKmh => Value::Real(real(&args[0]) * 1.852),
        SpeedKmhToKnot => Value::Real(real(&args[0]) / 1.852),
        PaToBar => Value::Real(real(&args[0]) / 100_000.0),
        BarToPa => Value::Real(real(&args[0]) * 100_000.0),
        PsiToBar => Value::Real(real(&args[0]) * 0.068_947_57),
        BarToPsi => Value::Real(real(&args[0]) / 0.068_947_57),
        AtmToBar => Value::Real(real(&args[0]) * 1.013_25),
        BarToAtm => Value::Real(real(&args[0]) / 1.013_25),
        JouleToCal => Value::Real(real(&args[0]) * 0.239_005_7),
        CalToJoule => Value::Real(real(&args[0]) / 0.239_005_7),
        KwhToJoule => Value::Real(real(&args[0]) * 3_600_000.0),
        JouleToKwh => Value::Real(real(&args[0]) / 3_600_000.0),
        WattToHp => Value::Real(real(&args[0]) * 0.001_341_022),
        HpToWatt => Value::Real(real(&args[0]) / 0.001_341_022),
        MpgToL100 => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("fuel.mpg_to_l100km: miles per gallon is zero".to_string()); }
            Value::Real(235.214_583 / x)
        }
        L100ToMpg => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("fuel.l100km_to_mpg: liters per 100km is zero".to_string()); }
            Value::Real(235.214_583 / x)
        }
        NewtonToLbf => Value::Real(real(&args[0]) * 0.224_808_94),
        LbfToNewton => Value::Real(real(&args[0]) / 0.224_808_94),
        NmToLbft => Value::Real(real(&args[0]) * 0.737_562_15),
        LbftToNm => Value::Real(real(&args[0]) / 0.737_562_15),
        MbpsToMbs => Value::Real(real(&args[0]) / 8.0),
        MbsToMbps => Value::Real(real(&args[0]) * 8.0),
        HzToRpm => Value::Real(real(&args[0]) * 60.0),
        RpmToHz => Value::Real(real(&args[0]) / 60.0),
        TbspToMl => Value::Real(real(&args[0]) * 14.786_765),
        MlToTbsp => Value::Real(real(&args[0]) / 14.786_765),
        TspToMl => Value::Real(real(&args[0]) * 4.928_922),
        MlToTsp => Value::Real(real(&args[0]) / 4.928_922),
        NmiToKm => Value::Real(real(&args[0]) * 1.852),
        KmToNmi => Value::Real(real(&args[0]) / 1.852),
        AuToKm => Value::Real(real(&args[0]) * 149_597_870.7),
        KmToAu => Value::Real(real(&args[0]) / 149_597_870.7),
        LyToKm => Value::Real(real(&args[0]) * 9.460_730_472e12),
        KmToLy => Value::Real(real(&args[0]) / 9.460_730_472e12),
        StoneToKg => Value::Real(real(&args[0]) * 6.350_293_18),
        KgToStone => Value::Real(real(&args[0]) / 6.350_293_18),
        Gcm3ToKgm3 => Value::Real(real(&args[0]) * 1000.0),
        Kgm3ToGcm3 => Value::Real(real(&args[0]) / 1000.0),
        GainToDb => Value::Real(20.0 * real(&args[0]).log10()),
        DbToGain => Value::Real(10f64.powf(real(&args[0]) / 20.0)),
        BpmToMs => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("tempo.bpm_to_ms: the tempo is zero".to_string()); }
            Value::Real(60_000.0 / x)
        }
        MsToBpm => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("tempo.ms_to_bpm: the duration is zero".to_string()); }
            Value::Real(60_000.0 / x)
        }
        DegToGrad => Value::Real(real(&args[0]) * 10.0 / 9.0),
        GradToDeg => Value::Real(real(&args[0]) * 9.0 / 10.0),
        RadToGrad => Value::Real(real(&args[0]) * 200.0 / std::f64::consts::PI),
        GradToRad => Value::Real(real(&args[0]) * std::f64::consts::PI / 200.0),
        HoursToSeconds => Value::Real(real(&args[0]) * 3600.0),
        SecondsToHours => Value::Real(real(&args[0]) / 3600.0),
        DaysToSeconds => Value::Real(real(&args[0]) * 86400.0),
        SecondsToDays => Value::Real(real(&args[0]) / 86400.0),
        MinkmToKmh => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("pace.minkm_to_kmh: the pace is zero".to_string()); }
            Value::Real(60.0 / x)
        }
        KmhToMinkm => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("pace.kmh_to_minkm: the speed is zero".to_string()); }
            Value::Real(60.0 / x)
        }

        // batch B: maths
        HyperSinh => Value::Real(real(&args[0]).sinh()),
        HyperCosh => Value::Real(real(&args[0]).cosh()),
        HyperTanh => Value::Real(real(&args[0]).tanh()),
        AhyperAsinh => Value::Real(real(&args[0]).asinh()),
        AhyperAcosh => Value::Real(real(&args[0]).acosh()),
        AhyperAtanh => Value::Real(real(&args[0]).atanh()),
        LogsTwo => Value::Real(real(&args[0]).log2()),
        LogsTen => Value::Real(real(&args[0]).log10()),
        LogsNatural => Value::Real(real(&args[0]).ln()),
        TrigxSec => Value::Real(1.0 / real(&args[0]).to_radians().cos()),
        TrigxCsc => Value::Real(1.0 / real(&args[0]).to_radians().sin()),
        TrigxCot => Value::Real(1.0 / real(&args[0]).to_radians().tan()),
        ChooseCombinations => Value::Int(combinations(int(&args[0]), int(&args[1]))?),
        ChoosePermutations => Value::Int(permutations(int(&args[0]), int(&args[1]))?),
        SeqTriangular => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n + 1).ok_or(OVERFLOW)? / 2)
        }
        SeqSquare => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n).ok_or(OVERFLOW)?)
        }
        SeqCube => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n).and_then(|x| x.checked_mul(n)).ok_or(OVERFLOW)?)
        }
        SeqCatalan => Value::Int(catalan(int(&args[0]))?),
        SeqTetrahedral => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n + 1).and_then(|x| x.checked_mul(n + 2)).ok_or(OVERFLOW)? / 6)
        }
        ModularPow => Value::Int(mod_pow(int(&args[0]), int(&args[1]), int(&args[2]))?),
        LerpBetween => {
            let (a, c, t) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(a + (c - a) * t)
        }
        LerpMapRange => {
            let (x, a, b2, c, d) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
            if a == b2 {
                return Err("lerp.map_range: the input range is empty".to_string());
            }
            Value::Real(c + (x - a) * (d - c) / (b2 - a))
        }
        RoundingUp => to_int_checked(real(&args[0]).ceil())?,
        RoundingDown => to_int_checked(real(&args[0]).floor())?,
        RoundingHalfUp => to_int_checked((real(&args[0]) + 0.5).floor())?,
        RoundingToMultiple => {
            let (x, m) = (real(&args[0]), real(&args[1]));
            if m == 0.0 {
                return Err("rounding.to_multiple: the step is zero".to_string());
            }
            Value::Real((x / m).round() * m)
        }
        Vec2Length => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        Vec2Dot => Value::Real(real(&args[0]) * real(&args[2]) + real(&args[1]) * real(&args[3])),
        Vec2Angle => Value::Real(real(&args[1]).atan2(real(&args[0])).to_degrees()),
        Vec3Length => {
            let (x, y, z) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real((x * x + y * y + z * z).sqrt())
        }
        Vec3Dot => Value::Real(
            real(&args[0]) * real(&args[3]) + real(&args[1]) * real(&args[4]) + real(&args[2]) * real(&args[5]),
        ),
        ComplexAbs => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        ComplexArg => Value::Real(real(&args[1]).atan2(real(&args[0]))),
        ParityIsEven => Value::Bool(int(&args[0]) % 2 == 0),
        ParityIsOdd => Value::Bool(int(&args[0]) % 2 != 0),
        DivisibleBy => {
            let d = int(&args[1]);
            if d == 0 {
                return Err("divisible.by: cannot divide by zero".to_string());
            }
            Value::Bool(int(&args[0]) % d == 0)
        }
        DigitsSum => Value::Int(int(&args[0]).unsigned_abs().to_string().bytes().map(|b| (b - b'0') as i64).sum()),
        DigitsCount => Value::Int(int(&args[0]).unsigned_abs().to_string().len() as i64),
        DigitsReverse => {
            let n = int(&args[0]);
            let s: String = n.unsigned_abs().to_string().chars().rev().collect();
            let v: i64 = s.parse().map_err(|_| OVERFLOW.to_string())?;
            Value::Int(if n < 0 { -v } else { v })
        }
        CollatzSteps => Value::Int(collatz(int(&args[0]))?),
        PerfectIs => Value::Bool(is_perfect(int(&args[0]))),
        ArmstrongIs => Value::Bool(is_armstrong(int(&args[0]))),
        PalindromeText => {
            let clean: String =
                text(&args[0]).chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
            Value::Bool(clean.chars().eq(clean.chars().rev()))
        }
        PalindromeNumber => {
            let s = int(&args[0]).unsigned_abs().to_string();
            Value::Bool(s.chars().eq(s.chars().rev()))
        }
        RangeContains => {
            let (x, lo, hi) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Bool(x >= lo && x <= hi)
        }
        PointDistance => {
            let (x1, y1, x2, y2) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
            Value::Real((x2 - x1).hypot(y2 - y1))
        }
        PointManhattan => {
            Value::Real((real(&args[2]) - real(&args[0])).abs() + (real(&args[3]) - real(&args[1])).abs())
        }
        SlopeBetween => {
            let (x1, y1, x2, y2) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
            if x1 == x2 {
                return Err("slope.between: the line is vertical".to_string());
            }
            Value::Real((y2 - y1) / (x2 - x1))
        }
        RootSquare => Value::Real(real(&args[0]).sqrt()),
        RootCube => Value::Real(real(&args[0]).cbrt()),
        RootNth => {
            let (x, n) = (real(&args[0]), real(&args[1]));
            if n == 0.0 {
                return Err("root.nth: the index is zero".to_string());
            }
            Value::Real(x.powf(1.0 / n))
        }

        // batch C: more text
        CountWords => Value::Int(text(&args[0]).split_whitespace().count() as i64),
        CountLines => {
            let s = text(&args[0]);
            Value::Int(if s.is_empty() { 0 } else { s.lines().count() as i64 })
        }
        CountChars => Value::Int(text(&args[0]).chars().count() as i64),
        CountVowels => Value::Int(text(&args[0]).chars().filter(|c| "aeiouAEIOU".contains(*c)).count() as i64),
        TruncateTo => {
            let (s, n) = (text(&args[0]), int(&args[1]).max(0) as usize);
            if s.chars().count() <= n {
                string(s)
            } else {
                string(s.chars().take(n).collect::<String>() + "…")
            }
        }
        TruncateWords => {
            let (s, n) = (text(&args[0]), int(&args[1]).max(0) as usize);
            let ws: Vec<&str> = s.split_whitespace().collect();
            if ws.len() <= n {
                string(s)
            } else {
                string(ws[..n].join(" ") + " …")
            }
        }
        IndentBy => {
            let (s, n) = (text(&args[0]), int(&args[1]).max(0) as usize);
            let pad = " ".repeat(n);
            string(s.lines().map(|l| format!("{}{}", pad, l)).collect::<Vec<_>>().join("\n"))
        }
        StripPrefix => {
            let (s, p) = (text(&args[0]), text(&args[1]));
            string(s.strip_prefix(p).unwrap_or(s))
        }
        StripSuffix => {
            let (s, p) = (text(&args[0]), text(&args[1]));
            string(s.strip_suffix(p).unwrap_or(s))
        }
        MaskAll => string(text(&args[0]).chars().map(|_| '*').collect::<String>()),
        MaskMiddle => string(mask_middle(text(&args[0]), int(&args[1]))),
        InitialsOf | AcronymOf => string(
            text(&args[0]).split_whitespace().filter_map(|w| w.chars().next()).map(|c| c.to_ascii_uppercase()).collect::<String>(),
        ),
        PigLatin => string(pig_latin(text(&args[0]))),
        AtbashApply => string(atbash(text(&args[0]))),
        VigenereEncode => string(vigenere(text(&args[0]), text(&args[1]), true)?),
        VigenereDecode => string(vigenere(text(&args[0]), text(&args[1]), false)?),
        Rot47Apply => string(rot47(text(&args[0]))),
        BinEncode => string(text(&args[0]).bytes().map(|b| format!("{:08b}", b)).collect::<Vec<_>>().join(" ")),
        BinDecode => string(bin_decode(text(&args[0]))?),
        Base32Encode => string(base32_encode(text(&args[0]).as_bytes())),
        Base32Decode => string(base32_decode(text(&args[0]))?),
        HtmlEscape => string(html_escape(text(&args[0]))),
        HtmlUnescape => string(html_unescape(text(&args[0]))),
        ShoutIt => string(format!("{}!", text(&args[0]).to_uppercase())),
        ShoutWhisper => string(text(&args[0]).to_lowercase()),
        WordsReverse => string(text(&args[0]).split_whitespace().rev().collect::<Vec<_>>().join(" ")),
        WordsFirst => string(text(&args[0]).split_whitespace().next().unwrap_or("")),
        WordsLast => string(text(&args[0]).split_whitespace().last().unwrap_or("")),
        UnicodeCodepoint => match text(&args[0]).chars().next() {
            Some(c) => Value::Int(c as i64),
            None => return Err("unicode.codepoint: the string is empty".to_string()),
        },
        UnicodeFromCodepoint => {
            let n = int(&args[0]);
            let c = u32::try_from(n)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("unicode.from_codepoint: {} is not a character", n))?;
            string(c.to_string())
        }
        UnicodeSize => Value::Int(text(&args[0]).chars().count() as i64),
        NormalizeSpaces => string(text(&args[0]).split_whitespace().collect::<Vec<_>>().join(" ")),
        AnagramOf => Value::Bool(anagram(text(&args[0]), text(&args[1]))),
        FizzbuzzAt => {
            let n = int(&args[0]);
            string(match (n % 3, n % 5) {
                (0, 0) => "FizzBuzz".to_string(),
                (0, _) => "Fizz".to_string(),
                (_, 0) => "Buzz".to_string(),
                _ => n.to_string(),
            })
        }
        QuoteAdd => string(format!("\"{}\"", text(&args[0]))),
        RleEncode => string(rle_encode(text(&args[0]))),
        RleDecode => string(rle_decode(text(&args[0]))?),
        WrapAt => string(wrap_at(text(&args[0]), int(&args[1]))),
        SpongebobMock => string(spongebob(text(&args[0]))),
        ClapText => string(text(&args[0]).split_whitespace().collect::<Vec<_>>().join(" 👏 ")),

        // batch D: science, finance, geometry, dates
        BmiValue => {
            let (w, h) = (real(&args[0]), real(&args[1]));
            if h <= 0.0 {
                return Err("bmi.value: the height must be positive".to_string());
            }
            Value::Real(w / (h * h))
        }
        BmiCategory => string(bmi_category(real(&args[0]))),
        OhmVoltage => Value::Real(real(&args[0]) * real(&args[1])),
        OhmCurrent => {
            let r = real(&args[1]);
            if r == 0.0 {
                return Err("ohm.current: the resistance is zero".to_string());
            }
            Value::Real(real(&args[0]) / r)
        }
        OhmResistance => {
            let i = real(&args[1]);
            if i == 0.0 {
                return Err("ohm.resistance: the current is zero".to_string());
            }
            Value::Real(real(&args[0]) / i)
        }
        OhmPower => Value::Real(real(&args[0]) * real(&args[1])),
        PhysicsForce => Value::Real(real(&args[0]) * real(&args[1])),
        PhysicsKinetic => {
            let (m, v) = (real(&args[0]), real(&args[1]));
            Value::Real(0.5 * m * v * v)
        }
        PhysicsMomentum => Value::Real(real(&args[0]) * real(&args[1])),
        WaveFrequency => {
            let wl = real(&args[1]);
            if wl == 0.0 {
                return Err("wave.frequency: the wavelength is zero".to_string());
            }
            Value::Real(real(&args[0]) / wl)
        }
        WaveWavelength => {
            let f = real(&args[1]);
            if f == 0.0 {
                return Err("wave.wavelength: the frequency is zero".to_string());
            }
            Value::Real(real(&args[0]) / f)
        }
        WavePeriod => {
            let f = real(&args[0]);
            if f == 0.0 {
                return Err("wave.period: the frequency is zero".to_string());
            }
            Value::Real(1.0 / f)
        }
        GravityOn => Value::Real(planet_gravity(text(&args[0]))?),
        InterestSimple => Value::Real(real(&args[0]) * real(&args[1]) / 100.0 * real(&args[2])),
        InterestCompound => {
            let (p, r, t) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(p * (1.0 + r / 100.0).powf(t) - p)
        }
        LoanPayment => Value::Real(loan_payment(real(&args[0]), real(&args[1]), int(&args[2]))?),
        TipAmount => Value::Real(real(&args[0]) * real(&args[1]) / 100.0),
        TipTotal => Value::Real(real(&args[0]) * (1.0 + real(&args[1]) / 100.0)),
        DiscountApply => Value::Real(real(&args[0]) * (1.0 - real(&args[1]) / 100.0)),
        DiscountSaved => Value::Real(real(&args[0]) * real(&args[1]) / 100.0),
        RoiPercent => {
            let c = real(&args[1]);
            if c == 0.0 {
                return Err("roi.percent: the cost is zero".to_string());
            }
            Value::Real(real(&args[0]) / c * 100.0)
        }
        MoneyFormat => string(money_format(real(&args[0]))),
        BearingBetween => Value::Real(bearing(real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]))),
        TriangleHypotenuse => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        TriangleHeron => Value::Real(heron(real(&args[0]), real(&args[1]), real(&args[2]))?),
        TriangleIsRight => {
            let mut s = [real(&args[0]), real(&args[1]), real(&args[2])];
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            Value::Bool((s[0] * s[0] + s[1] * s[1] - s[2] * s[2]).abs() < 1e-9)
        }
        CircleArea => Value::Real(std::f64::consts::PI * real(&args[0]).powi(2)),
        CircleCircumference => Value::Real(2.0 * std::f64::consts::PI * real(&args[0])),
        CircleDiameter => Value::Real(2.0 * real(&args[0])),
        RectangleArea => Value::Real(real(&args[0]) * real(&args[1])),
        RectanglePerimeter => Value::Real(2.0 * (real(&args[0]) + real(&args[1]))),
        RectangleDiagonal => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        SphereVolume => Value::Real(4.0 / 3.0 * std::f64::consts::PI * real(&args[0]).powi(3)),
        SphereSurface => Value::Real(4.0 * std::f64::consts::PI * real(&args[0]).powi(2)),
        CylinderVolume => {
            let (r, h) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * r * r * h)
        }
        CylinderSurface => {
            let (r, h) = (real(&args[0]), real(&args[1]));
            Value::Real(2.0 * std::f64::consts::PI * r * (r + h))
        }
        ConeVolume => {
            let (r, h) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * r * r * h / 3.0)
        }
        ConeSurface => {
            let (r, h) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * r * (r + (r * r + h * h).sqrt()))
        }
        WeekdayOf => string(weekday_of(int(&args[0]))?),
        MonthName => string(month_name(int(&args[0]))?),
        MonthDays => Value::Int(days_in_month(int(&args[0]), int(&args[1]))?),
        LeapYear => Value::Bool(is_leap(int(&args[0]))),
        AgeYears => Value::Int(age_years(int(&args[0]), int(&args[1]))),
        ZodiacSign => string(zodiac_sign(int(&args[0]), int(&args[1]))?),
        SeasonOf => string(season_of(int(&args[0]))?),
        GradeLetter => string(grade_letter(real(&args[0]))),
        LogicXor => Value::Bool(boolv(&args[0]) ^ boolv(&args[1])),
        LogicImplies => Value::Bool(!boolv(&args[0]) || boolv(&args[1])),
        LogicNand => Value::Bool(!(boolv(&args[0]) && boolv(&args[1]))),
        LogicNor => Value::Bool(!(boolv(&args[0]) || boolv(&args[1]))),
        YesnoOf => string(if boolv(&args[0]) { "yes" } else { "no" }),
        ScientificNotation => string(format!("{:e}", real(&args[0]))),
        FractionToDecimal => {
            let d = int(&args[1]);
            if d == 0 {
                return Err("fraction.to_decimal: the denominator is zero".to_string());
            }
            Value::Real(int(&args[0]) as f64 / d as f64)
        }
        FractionSimplify => string(fraction_simplify(int(&args[0]), int(&args[1]))?),
        HeartrateMax => Value::Int((220 - int(&args[0])).max(0)),
        HeartrateZone => {
            let (age, pct) = (int(&args[0]), int(&args[1]));
            Value::Int((220 - age).max(0) * pct / 100)
        }

        // batch E: hashing, validation, randomness, ids, utilities
        FnvHash => Value::Int(fnv1a(text(&args[0]))),
        Djb2Hash => Value::Int(djb2(text(&args[0]))),
        SdbmHash => Value::Int(sdbm(text(&args[0]))),
        AdlerValue => Value::Int(adler32(text(&args[0]))),
        Ipv4Valid => Value::Bool(ipv4_valid(text(&args[0]))),
        MacValid => Value::Bool(regex_is(r"^([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}$", text(&args[0]))),
        HexcolorValid => Value::Bool(regex_is(r"^#?([0-9A-Fa-f]{3}|[0-9A-Fa-f]{6})$", text(&args[0]))),
        WeburlValid => Value::Bool(regex_is(r"^https?://[^\s]+$", text(&args[0]))),
        DiceRoll => {
            let s = int(&args[0]);
            if s < 1 {
                return Err("dice.roll: the die needs at least 1 side".to_string());
            }
            Value::Int(rng_range(1, s))
        }
        DiceMany => {
            let (count, sides) = (int(&args[0]), int(&args[1]));
            if count < 0 {
                return Err("dice.many: the count cannot be negative".to_string());
            }
            if sides < 1 {
                return Err("dice.many: the die needs at least 1 side".to_string());
            }
            let mut total = 0i64;
            for _ in 0..count {
                total += rng_range(1, sides);
            }
            Value::Int(total)
        }
        CoinFlip => Value::Bool(rng_u64() & 1 == 0),
        PickBetween => {
            let (lo, hi) = (int(&args[0]), int(&args[1]));
            if lo > hi {
                return Err("pick.between: the minimum is greater than the maximum".to_string());
            }
            Value::Int(rng_range(lo, hi))
        }
        PickChance => Value::Bool((rng_u64() as f64 / u64::MAX as f64) < real(&args[0])),
        TokenHex => string(rng_hex(int(&args[0]))?),
        PasswordMake => string(rng_password(int(&args[0]))?),
        NanoidMake => string(rng_nanoid()),
        OtpCode => Value::Int(rng_range(100_000, 999_999)),
        LoremWords => string(lorem_words(int(&args[0]))),
        RandcolorHex => string(format!("#{:06x}", rng_u64() as u32 & 0xFF_FFFF)),
        FactorCount => Value::Int(divisor_count(int(&args[0]))?),
        FactorSum => Value::Int(divisor_sum(int(&args[0]))?),
        TotientOf => Value::Int(totient(int(&args[0]))?),
        CompareOf => {
            let (a, b) = (real(&args[0]), real(&args[1]));
            Value::Int(if a < b { -1 } else if a > b { 1 } else { 0 })
        }
        ClampintBetween => {
            let (x, lo, hi) = (int(&args[0]), int(&args[1]), int(&args[2]));
            Value::Int(x.max(lo).min(hi))
        }
        PowSquare => Value::Real(real(&args[0]).powi(2)),
        PowCube => Value::Real(real(&args[0]).powi(3)),
        PowOf => Value::Real(real(&args[0]).powf(real(&args[1]))),
        CapsFirst => string(caps_first(text(&args[0]))),
        SwapcaseApply => string(
            text(&args[0])
                .chars()
                .map(|c| if c.is_uppercase() { c.to_ascii_lowercase() } else { c.to_ascii_uppercase() })
                .collect::<String>(),
        ),
        ConsonantsCount => Value::Int(
            text(&args[0]).chars().filter(|c| c.is_ascii_alphabetic() && !"aeiouAEIOU".contains(*c)).count() as i64,
        ),
        Point3Distance => {
            let d = |a: usize, b: usize| real(&args[b]) - real(&args[a]);
            Value::Real((d(0, 3).powi(2) + d(1, 4).powi(2) + d(2, 5).powi(2)).sqrt())
        }
        MidpointOf => Value::Real((real(&args[0]) + real(&args[1])) / 2.0),
        IsqrtOf => {
            let n = int(&args[0]);
            if n < 0 {
                return Err("isqrt.of: the number is negative".to_string());
            }
            let mut r = (n as f64).sqrt() as i64;
            while r > 0 && r.saturating_mul(r) > n {
                r -= 1;
            }
            while (r + 1).saturating_mul(r + 1) <= n {
                r += 1;
            }
            Value::Int(r)
        }
        CycleIndex => {
            let len = int(&args[1]);
            if len <= 0 {
                return Err("cycle.index: the length must be positive".to_string());
            }
            Value::Int(int(&args[0]).rem_euclid(len))
        }
        DigitrootOf => {
            let n = int(&args[0]).unsigned_abs();
            Value::Int(if n == 0 { 0 } else { 1 + ((n - 1) % 9) as i64 })
        }

        // batch F: advanced geometry, finance, weather, physics
        TrapezoidArea => Value::Real((real(&args[0]) + real(&args[1])) / 2.0 * real(&args[2])),
        EllipseArea => Value::Real(std::f64::consts::PI * real(&args[0]) * real(&args[1])),
        EllipsePerimeter => {
            let (a, b) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * (3.0 * (a + b) - ((3.0 * a + b) * (a + 3.0 * b)).sqrt()))
        }
        PentagonArea => {
            let s = real(&args[0]);
            Value::Real(5.0 * s * s / (4.0 * (std::f64::consts::PI / 5.0).tan()))
        }
        HexagonArea => {
            let s = real(&args[0]);
            Value::Real(3.0 * 3f64.sqrt() / 2.0 * s * s)
        }
        PolygonInteriorAngle => {
            let n = int(&args[0]);
            if n < 3 {
                return Err("polygon.interior_angle: a polygon needs at least 3 sides".to_string());
            }
            Value::Real((n - 2) as f64 * 180.0 / n as f64)
        }
        PolygonArea => {
            let n = int(&args[0]);
            let s = real(&args[1]);
            if n < 3 {
                return Err("polygon.area: a polygon needs at least 3 sides".to_string());
            }
            Value::Real(n as f64 * s * s / (4.0 * (std::f64::consts::PI / n as f64).tan()))
        }
        PyramidVolume => Value::Real(real(&args[0]) * real(&args[1]) / 3.0),
        PrismVolume => Value::Real(real(&args[0]) * real(&args[1])),
        TorusVolume => {
            let (big, small) = (real(&args[0]), real(&args[1]));
            Value::Real(2.0 * std::f64::consts::PI.powi(2) * big * small * small)
        }
        CuboidVolume => Value::Real(real(&args[0]) * real(&args[1]) * real(&args[2])),
        CuboidSurface => {
            let (l, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(2.0 * (l * w + l * h + w * h))
        }
        AnnulusArea => {
            let (o, i) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * (o * o - i * i))
        }
        SavingsFutureValue => {
            let (pmt, rate, n) = (real(&args[0]), real(&args[1]), int(&args[2]));
            if n < 0 {
                return Err("savings.future_value: the number of periods cannot be negative".to_string());
            }
            let r = rate / 100.0;
            let n = n as f64;
            Value::Real(if r == 0.0 { pmt * n } else { pmt * (((1.0 + r).powf(n) - 1.0) / r) })
        }
        DepreciationStraightLine => {
            let (cost, salvage, life) = (real(&args[0]), real(&args[1]), int(&args[2]));
            if life <= 0 {
                return Err("depreciation.straight_line: the life must be positive".to_string());
            }
            Value::Real((cost - salvage) / life as f64)
        }
        CentsToCents => to_int_checked((real(&args[0]) * 100.0).round())?,
        CentsFromCents => Value::Real(int(&args[0]) as f64 / 100.0),
        MarkupPrice => Value::Real(real(&args[0]) * (1.0 + real(&args[1]) / 100.0)),
        MarkupMargin => {
            let (price, cost) = (real(&args[0]), real(&args[1]));
            if price == 0.0 {
                return Err("markup.margin: the price is zero".to_string());
            }
            Value::Real((price - cost) / price * 100.0)
        }
        BmrMen => {
            let (w, h, age) = (real(&args[0]), real(&args[1]), int(&args[2]));
            Value::Real(88.362 + 13.397 * w + 4.799 * h - 5.677 * age as f64)
        }
        BmrWomen => {
            let (w, h, age) = (real(&args[0]), real(&args[1]), int(&args[2]));
            Value::Real(447.593 + 9.247 * w + 3.098 * h - 4.330 * age as f64)
        }
        WindChill => {
            let (t, v) = (real(&args[0]), real(&args[1]));
            let vp = v.powf(0.16);
            Value::Real(13.12 + 0.6215 * t - 11.37 * vp + 0.3965 * t * vp)
        }
        HeatIndex => Value::Real(heat_index(real(&args[0]), real(&args[1]))),
        DewpointOf => {
            let (t, rh) = (real(&args[0]), real(&args[1]));
            if rh <= 0.0 {
                return Err("dewpoint.of: the humidity must be positive".to_string());
            }
            let (a, b) = (17.27, 237.7);
            let alpha = (a * t) / (b + t) + (rh / 100.0).ln();
            Value::Real((b * alpha) / (a - alpha))
        }
        QuadraticDiscriminant => {
            let (a, b, c) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(b * b - 4.0 * a * c)
        }
        QuadraticRoot1 => Value::Real(quadratic_root(real(&args[0]), real(&args[1]), real(&args[2]), true)?),
        QuadraticRoot2 => Value::Real(quadratic_root(real(&args[0]), real(&args[1]), real(&args[2]), false)?),
        MotionSpeed => {
            let t = real(&args[1]);
            if t == 0.0 {
                return Err("motion.speed: the time is zero".to_string());
            }
            Value::Real(real(&args[0]) / t)
        }
        MotionTime => {
            let s = real(&args[1]);
            if s == 0.0 {
                return Err("motion.time: the speed is zero".to_string());
            }
            Value::Real(real(&args[0]) / s)
        }
        MotionDistance => Value::Real(real(&args[0]) * real(&args[1])),
        AprToMonthly => Value::Real(((1.0 + real(&args[0]) / 100.0).powf(1.0 / 12.0) - 1.0) * 100.0),
        AprToAnnual => Value::Real(((1.0 + real(&args[0]) / 100.0).powi(12) - 1.0) * 100.0),

        // module "grid": fast contiguous float arrays
        GridRange => Value::Int(grid_new((0..int(&args[0]).max(0)).map(|i| i as f64).collect())),
        GridZeros => Value::Int(grid_new(vec![0.0; int(&args[0]).max(0) as usize])),
        GridFill => Value::Int(grid_new(vec![real(&args[1]); int(&args[0]).max(0) as usize])),
        GridOf => Value::Int(grid_new(seq(&args[0]).borrow().iter().map(real).collect())),
        GridSize => Value::Int(grid_read(int(&args[0]), |v| v.len() as i64)?),
        GridGet => {
            let i = int(&args[1]);
            let x = grid_read(int(&args[0]), |v| if i < 1 || i as usize > v.len() { None } else { Some(v[i as usize - 1]) })?;
            Value::Real(x.ok_or_else(|| format!("grid.get: there is no element number {}", i))?)
        }
        GridSet => {
            let (i, val) = (int(&args[1]), real(&args[2]));
            GRIDS.with(|g| {
                let mut g = g.borrow_mut();
                let v = g.get_mut(int(&args[0]) as usize).ok_or("grid: invalid handle")?;
                if i < 1 || i as usize > v.len() {
                    return Err(format!("grid.set: there is no element number {}", i));
                }
                v[i as usize - 1] = val;
                Ok::<(), String>(())
            })?;
            Value::Null
        }
        GridFree => {
            GRIDS.with(|g| {
                if let Some(v) = g.borrow_mut().get_mut(int(&args[0]) as usize) {
                    *v = Vec::new();
                }
            });
            Value::Null
        }
        GridToList => {
            let items = grid_read(int(&args[0]), |v| v.iter().map(|&x| Value::Real(x)).collect::<Vec<_>>())?;
            list(items)
        }
        GridAdd => Value::Int(grid_zip(int(&args[0]), int(&args[1]), |x, y| x + y)?),
        GridSub => Value::Int(grid_zip(int(&args[0]), int(&args[1]), |x, y| x - y)?),
        GridMul => Value::Int(grid_zip(int(&args[0]), int(&args[1]), |x, y| x * y)?),
        GridDiv => Value::Int(grid_zip(int(&args[0]), int(&args[1]), |x, y| x / y)?),
        GridScale => {
            let k = real(&args[1]);
            Value::Int(grid_map(int(&args[0]), move |x| x * k)?)
        }
        GridOffset => {
            let k = real(&args[1]);
            Value::Int(grid_map(int(&args[0]), move |x| x + k)?)
        }
        GridSqrt => Value::Int(grid_map(int(&args[0]), |x| x.sqrt())?),
        GridSquare => Value::Int(grid_map(int(&args[0]), |x| x * x)?),
        GridAbs => Value::Int(grid_map(int(&args[0]), |x| x.abs())?),
        GridSum => Value::Real(grid_read(int(&args[0]), |v| v.par_iter().sum())?),
        GridMean => {
            let (s, n) = grid_read(int(&args[0]), |v| (v.par_iter().sum::<f64>(), v.len()))?;
            if n == 0 {
                return Err("grid.mean: the grid is empty".to_string());
            }
            Value::Real(s / n as f64)
        }
        GridMax => Value::Real(grid_read(int(&args[0]), |v| v.par_iter().cloned().reduce(|| f64::NEG_INFINITY, f64::max))?),
        GridMin => Value::Real(grid_read(int(&args[0]), |v| v.par_iter().cloned().reduce(|| f64::INFINITY, f64::min))?),
        GridDot => grid_dot(int(&args[0]), int(&args[1]))?,
        GridSumSquares => Value::Real(grid_read(int(&args[0]), |v| v.par_iter().map(|&x| x * x).sum())?),
        GridNorm => Value::Real(grid_read(int(&args[0]), |v| v.par_iter().map(|&x| x * x).sum::<f64>().sqrt())?),

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
