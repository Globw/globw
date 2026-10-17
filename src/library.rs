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
