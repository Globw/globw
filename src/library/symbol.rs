//! The "symbol" module's helpers: symbolic mathematics (a tiny CAS). The
//! builtins live in `call()`; this holds the expression store and the
//! differentiate / simplify / evaluate / print passes.
use std::cell::RefCell;
use crate::vm::format_real;

// Un ops: 0 neg, 1 sin, 2 cos, 3 exp, 4 ln. Bin ops: 0 add, 1 sub, 2 mul, 3 div, 4 pow.
#[derive(Clone)]
pub(super) enum SNode {
    Const(f64),
    Var(String),
    Un(u8, usize),
    Bin(u8, usize, usize),
}

thread_local! {
    pub(super) static SYM: RefCell<Vec<SNode>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn s_push(v: &mut Vec<SNode>, n: SNode) -> usize {
    v.push(n);
    v.len() - 1
}

pub(super) fn s_check(len: usize, h: i64) -> Result<usize, String> {
    usize::try_from(h).ok().filter(|&i| i < len).ok_or_else(|| "symbol: invalid handle".to_string())
}

pub(super) fn s_diff(v: &mut Vec<SNode>, e: usize, var: &str) -> usize {
    match v[e].clone() {
        SNode::Const(_) => s_push(v, SNode::Const(0.0)),
        SNode::Var(n) => s_push(v, SNode::Const(if n == var { 1.0 } else { 0.0 })),
        SNode::Un(op, a) => {
            let da = s_diff(v, a, var);
            match op {
                0 => s_push(v, SNode::Un(0, da)),
                1 => {
                    let c = s_push(v, SNode::Un(2, a));
                    s_push(v, SNode::Bin(2, c, da))
                }
                2 => {
                    let s = s_push(v, SNode::Un(1, a));
                    let ns = s_push(v, SNode::Un(0, s));
                    s_push(v, SNode::Bin(2, ns, da))
                }
                3 => {
                    let ex = s_push(v, SNode::Un(3, a));
                    s_push(v, SNode::Bin(2, ex, da))
                }
                _ => s_push(v, SNode::Bin(3, da, a)),
            }
        }
        SNode::Bin(op, a, b) => {
            let da = s_diff(v, a, var);
            let db = s_diff(v, b, var);
            match op {
                0 => s_push(v, SNode::Bin(0, da, db)),
                1 => s_push(v, SNode::Bin(1, da, db)),
                2 => {
                    let m1 = s_push(v, SNode::Bin(2, da, b));
                    let m2 = s_push(v, SNode::Bin(2, a, db));
                    s_push(v, SNode::Bin(0, m1, m2))
                }
                3 => {
                    let m1 = s_push(v, SNode::Bin(2, da, b));
                    let m2 = s_push(v, SNode::Bin(2, a, db));
                    let num = s_push(v, SNode::Bin(1, m1, m2));
                    let den = s_push(v, SNode::Bin(2, b, b));
                    s_push(v, SNode::Bin(3, num, den))
                }
                _ => {
                    // a^b: a^b * (db*ln(a) + b*da/a)
                    let lna = s_push(v, SNode::Un(4, a));
                    let t1 = s_push(v, SNode::Bin(2, db, lna));
                    let dao = s_push(v, SNode::Bin(3, da, a));
                    let t2 = s_push(v, SNode::Bin(2, b, dao));
                    let sum = s_push(v, SNode::Bin(0, t1, t2));
                    s_push(v, SNode::Bin(2, e, sum))
                }
            }
        }
    }
}

pub(super) fn s_const(v: &[SNode], e: usize) -> Option<f64> {
    if let SNode::Const(c) = v[e] {
        Some(c)
    } else {
        None
    }
}

pub(super) fn s_simplify(v: &mut Vec<SNode>, e: usize) -> usize {
    match v[e].clone() {
        SNode::Const(_) | SNode::Var(_) => e,
        SNode::Un(op, a) => {
            let sa = s_simplify(v, a);
            if let Some(c) = s_const(v, sa) {
                let r = match op {
                    0 => -c,
                    1 => c.sin(),
                    2 => c.cos(),
                    3 => c.exp(),
                    _ => c.ln(),
                };
                return s_push(v, SNode::Const(r));
            }
            if op == 0 {
                if let SNode::Un(0, inner) = v[sa] {
                    return inner;
                }
            }
            s_push(v, SNode::Un(op, sa))
        }
        SNode::Bin(op, a, b) => {
            let sa = s_simplify(v, a);
            let sb = s_simplify(v, b);
            let (ca, cb) = (s_const(v, sa), s_const(v, sb));
            if let (Some(x), Some(y)) = (ca, cb) {
                let r = match op {
                    0 => x + y,
                    1 => x - y,
                    2 => x * y,
                    3 => x / y,
                    _ => x.powf(y),
                };
                return s_push(v, SNode::Const(r));
            }
            match op {
                0 => {
                    if ca == Some(0.0) {
                        return sb;
                    }
                    if cb == Some(0.0) {
                        return sa;
                    }
                }
                1 => {
                    if cb == Some(0.0) {
                        return sa;
                    }
                }
                2 => {
                    if ca == Some(0.0) || cb == Some(0.0) {
                        return s_push(v, SNode::Const(0.0));
                    }
                    if ca == Some(1.0) {
                        return sb;
                    }
                    if cb == Some(1.0) {
                        return sa;
                    }
                }
                3 => {
                    if ca == Some(0.0) {
                        return s_push(v, SNode::Const(0.0));
                    }
                    if cb == Some(1.0) {
                        return sa;
                    }
                }
                4 => {
                    if cb == Some(1.0) {
                        return sa;
                    }
                    if cb == Some(0.0) {
                        return s_push(v, SNode::Const(1.0));
                    }
                }
                _ => {}
            }
            s_push(v, SNode::Bin(op, sa, sb))
        }
    }
}

pub(super) fn s_eval(v: &[SNode], e: usize, var: &str, val: f64) -> Result<f64, String> {
    Ok(match &v[e] {
        SNode::Const(c) => *c,
        SNode::Var(n) => {
            if n == var {
                val
            } else {
                return Err(format!("symbol.eval: the expression still has a free variable \"{}\"", n));
            }
        }
        SNode::Un(op, a) => {
            let x = s_eval(v, *a, var, val)?;
            match op {
                0 => -x,
                1 => x.sin(),
                2 => x.cos(),
                3 => x.exp(),
                _ => x.ln(),
            }
        }
        SNode::Bin(op, a, b) => {
            let x = s_eval(v, *a, var, val)?;
            let y = s_eval(v, *b, var, val)?;
            match op {
                0 => x + y,
                1 => x - y,
                2 => x * y,
                3 => x / y,
                _ => x.powf(y),
            }
        }
    })
}

pub(super) fn s_text(v: &[SNode], e: usize) -> String {
    match &v[e] {
        SNode::Const(c) => format_real(*c),
        SNode::Var(n) => n.clone(),
        SNode::Un(op, a) => {
            let s = s_text(v, *a);
            match op {
                0 => format!("(-{})", s),
                1 => format!("sin({})", s),
                2 => format!("cos({})", s),
                3 => format!("exp({})", s),
                _ => format!("ln({})", s),
            }
        }
        SNode::Bin(op, a, b) => {
            let (x, y) = (s_text(v, *a), s_text(v, *b));
            let o = match op {
                0 => "+",
                1 => "-",
                2 => "*",
                3 => "/",
                _ => "^",
            };
            format!("({} {} {})", x, o, y)
        }
    }
}
