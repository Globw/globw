//! `format(value, spec)` and the `${value:spec}` form of string interpolation: a
//! small, Python-like format language for numbers and text.
//!
//! spec = [[fill]align][0][width][,][.precision][%]
//!   align      `<` left, `>` right, `^` centre (numbers default right, text left)
//!   0          pad a number with zeros after its sign, up to the width
//!   width      minimum length in characters
//!   ,          group the thousands of a number: 1,234,567
//!   .precision digits after the decimal point (rounded): .2 → 3.14
//!   %          a number as a percentage (×100, then `%`)

use crate::vm::{display, Value};

struct Spec {
    fill: char,
    align: Option<char>,
    zero: bool,
    width: usize,
    comma: bool,
    precision: Option<usize>,
    percent: bool,
}

fn parse(spec: &str) -> Result<Spec, String> {
    let bad = || {
        format!(
            "format: \"{}\" is not a valid format (examples: \".2\" two decimals, \">8\" right in 8, \",\" thousands, \"08.3\", \"^10\", \".1%\")",
            spec
        )
    };
    let c: Vec<char> = spec.chars().collect();
    let mut i = 0;
    let (mut fill, mut align) = (' ', None);
    if c.len() >= 2 && matches!(c[1], '<' | '>' | '^') {
        fill = c[0];
        align = Some(c[1]);
        i = 2;
    } else if !c.is_empty() && matches!(c[0], '<' | '>' | '^') {
        align = Some(c[0]);
        i = 1;
    }
    let mut zero = false;
    if i < c.len() && c[i] == '0' {
        zero = true;
        i += 1;
    }
    let mut width = 0usize;
    while i < c.len() && c[i].is_ascii_digit() {
        width = width.saturating_mul(10).saturating_add(c[i] as usize - '0' as usize);
        i += 1;
    }
    if width > 1000 {
        return Err("format: the width can be at most 1000".to_string());
    }
    let mut comma = false;
    if i < c.len() && c[i] == ',' {
        comma = true;
        i += 1;
    }
    let mut precision = None;
    if i < c.len() && c[i] == '.' {
        i += 1;
        let start = i;
        let mut p = 0usize;
        while i < c.len() && c[i].is_ascii_digit() {
            p = p.saturating_mul(10).saturating_add(c[i] as usize - '0' as usize);
            i += 1;
        }
        if i == start || p > 30 {
            return Err(bad());
        }
        precision = Some(p);
    }
    let mut percent = false;
    if i < c.len() && c[i] == '%' {
        percent = true;
        i += 1;
    }
    if i != c.len() {
        return Err(bad());
    }
    Ok(Spec { fill, align, zero, width, comma, precision, percent })
}

/// Groups the integer digits of a number written as text: "-1234.5" → "-1,234.5".
fn group(s: &str) -> String {
    let (sign, rest) = if let Some(r) = s.strip_prefix('-') { ("-", r) } else { ("", s) };
    let (int, frac) = match rest.find(['.', 'e', 'E']) {
        Some(k) => rest.split_at(k),
        None => (rest, ""),
    };
    if !int.chars().all(|ch| ch.is_ascii_digit()) {
        return s.to_string(); // inf, NaN
    }
    let mut out = String::new();
    for (k, ch) in int.chars().enumerate() {
        if k > 0 && (int.len() - k) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{sign}{out}{frac}")
}

/// Formats `v` by `spec`.
pub(super) fn format_value(v: &Value, spec: &str) -> Result<String, String> {
    let sp = parse(spec)?;
    let numeric = matches!(v, Value::Int(_) | Value::Real(_));
    if !numeric && (sp.comma || sp.precision.is_some() || sp.percent || sp.zero) {
        return Err(format!(
            "format: \"{}\" is a number format, but the value is not a number",
            spec
        ));
    }
    let mut body = match v {
        Value::Int(n) if sp.precision.is_none() && !sp.percent => n.to_string(),
        Value::Int(_) | Value::Real(_) => {
            let mut x = match v {
                Value::Int(n) => *n as f64,
                Value::Real(r) => *r,
                _ => unreachable!(),
            };
            if sp.percent {
                x *= 100.0;
            }
            match sp.precision {
                Some(p) => format!("{:.*}", p, x),
                None => display(&Value::Real(x)),
            }
        }
        other => display(other),
    };
    if sp.comma {
        body = group(&body);
    }
    if sp.percent {
        body.push('%');
    }
    let len = body.chars().count();
    if len >= sp.width {
        return Ok(body);
    }
    let pad = sp.width - len;
    if sp.zero && numeric && sp.align.is_none() {
        // zeros go after the sign: -0042
        let (sign, digits) = match body.strip_prefix('-') {
            Some(d) => ("-", d.to_string()),
            None => ("", body.clone()),
        };
        return Ok(format!("{sign}{}{digits}", "0".repeat(pad)));
    }
    let fill = if sp.zero && sp.align.is_none() { '0' } else { sp.fill };
    let f = |n: usize| fill.to_string().repeat(n);
    Ok(match sp.align.unwrap_or(if numeric { '>' } else { '<' }) {
        '<' => format!("{body}{}", f(pad)),
        '^' => format!("{}{body}{}", f(pad / 2), f(pad - pad / 2)),
        _ => format!("{}{body}", f(pad)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: Value, s: &str) -> String {
        format_value(&v, s).unwrap()
    }

    #[test]
    fn numbers() {
        assert_eq!(f(Value::Real(3.14159), ".2"), "3.14");
        assert_eq!(f(Value::Int(7), ".2"), "7.00");
        assert_eq!(f(Value::Int(1234567), ","), "1,234,567");
        assert_eq!(f(Value::Real(-1234567.891), ",.2"), "-1,234,567.89");
        assert_eq!(f(Value::Int(42), "05"), "00042");
        assert_eq!(f(Value::Int(-42), "05"), "-0042");
        assert_eq!(f(Value::Real(0.256), ".1%"), "25.6%");
        assert_eq!(f(Value::Int(5), ">4"), "   5");
        assert_eq!(f(Value::Int(5), "4"), "   5");
        assert_eq!(f(Value::Int(5), "<4"), "5   ");
        assert_eq!(f(Value::Real(2.5), "*^9.1"), "***2.5***");
    }

    #[test]
    fn text() {
        assert_eq!(f(Value::Str("ab".into()), "5"), "ab   ");
        assert_eq!(f(Value::Str("ab".into()), ">5"), "   ab");
        assert_eq!(f(Value::Str("ab".into()), "-^6"), "--ab--");
        assert!(format_value(&Value::Str("ab".into()), ".2").is_err());
        assert!(format_value(&Value::Int(1), "x").is_err());
    }
}
