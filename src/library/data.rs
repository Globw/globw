//! The "data" module's helpers: JSON parse/stringify and path access, bridging
//! serde_json and the interpreter's Value. The builtins live in `call()`.
use crate::vm::{Key as DictKey, Value};
use serde_json::Value as Json;
use super::text;


/// Parses the JSON text of a `data.*` call's first argument.
pub(super) fn json_root(v: &Value) -> Result<Json, String> {
    serde_json::from_str(text(v)).map_err(|e| format!("invalid JSON: {}", e))
}

/// Follows a path like `users[1].name` into a JSON value. Object members are
/// written `.name` (or just `name` at the start), array positions `[n]` and are
/// 1-based, as everywhere in Globw. An empty path is the whole value.
pub(super) fn json_at<'a>(root: &'a Json, path: &str) -> Result<&'a Json, String> {
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
pub(super) fn to_json_value(v: &Value) -> Result<Json, String> {
    Ok(match v {
        Value::Null => Json::Null,
        Value::Int(n) => Json::from(*n),
        Value::Real(r) => serde_json::Number::from_f64(*r)
            .map(Json::Number)
            .ok_or("cannot turn an infinite or not-a-number value into JSON")?,
        Value::Bool(b) => Json::Bool(*b),
        Value::Str(s) => Json::String(s.to_string()),
        Value::Seq(items) => Json::Array(items.borrow().iter().map(to_json_value).collect::<Result<_, _>>()?),
        Value::Floats(g, _) => Json::Array(
            g.borrow()
                .iter()
                .map(|&x| serde_json::Number::from_f64(x).map(Json::Number).ok_or_else(|| "a nungw value has no JSON form".to_string()))
                .collect::<Result<_, _>>()?,
        ),
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
        #[cfg(feature = "gpu")]
        Value::GpuFloats(..) => {
            return Err("a GPU-resident nungw has no JSON form; bring it back with nungw.to_cpu first".to_string());
        }
    })
}
