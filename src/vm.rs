//! The virtual machine: executes the register bytecode produced by the compiler.

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

use crate::bytecode::{Cmp, Module, Op};
use crate::errors::Error;

#[derive(Debug, Clone)]
pub enum Value {
    /// a register or element that has no value yet
    Null,
    Int(i64),
    Real(f64),
    Bool(bool),
    Str(Rc<str>),
    /// arrays and lists
    Seq(Rc<RefCell<Vec<Value>>>),
    Matrix(Rc<RefCell<Matrix>>),
}

#[derive(Debug)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Value>,
}

const OVERFLOW: &str = "number too large: the result exceeds the integer limits";

/// Shortest representation of a real, always with a decimal point.
pub fn format_real(r: f64) -> String {
    let s = r.to_string();
    if r.is_finite() && !s.contains('.') {
        s + ".0"
    } else {
        s
    }
}

fn display(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Real(r) => format_real(*r),
        Value::Bool(true) => "TRUE".to_string(),
        Value::Bool(false) => "FALSE".to_string(),
        Value::Str(s) => s.to_string(),
        Value::Null | Value::Seq(_) | Value::Matrix(_) => panic!("internal error: cannot display {:?}", v),
    }
}

#[inline(always)]
fn int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        other => panic!("internal error: expected an integer, found {:?}", other),
    }
}

#[inline(always)]
fn real(v: &Value) -> f64 {
    match v {
        Value::Real(r) => *r,
        other => panic!("internal error: expected a real, found {:?}", other),
    }
}

#[inline(always)]
fn boolean(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => panic!("internal error: expected a boolean, found {:?}", other),
    }
}

#[inline(always)]
fn string(v: &Value) -> &str {
    match v {
        Value::Str(s) => s,
        other => panic!("internal error: expected a string, found {:?}", other),
    }
}

fn compare<T: PartialOrd + ?Sized>(c: Cmp, a: &T, b: &T) -> bool {
    match c {
        Cmp::Eq => a == b,
        Cmp::NotEq => a != b,
        Cmp::Lt => a < b,
        Cmp::LtEq => a <= b,
        Cmp::Gt => a > b,
        Cmp::GtEq => a >= b,
    }
}

struct Frame {
    func: usize,
    ip: usize,
    base: usize,
    dst: u16,
}

pub fn run(module: &Module, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
    let mut vm = Vm {
        module,
        regs: Vec::with_capacity(1 << 12),
        globals: vec![Value::Null; module.globals.len()],
        frames: Vec::new(),
    };
    let result = vm.execute(input, output);
    let _ = output.flush();
    result
}

struct Vm<'m> {
    module: &'m Module,
    regs: Vec<Value>,
    globals: Vec<Value>,
    frames: Vec<Frame>,
}

impl<'m> Vm<'m> {
    fn error_at(&self, func: usize, ip: usize, message: impl Into<String>) -> Error {
        Error::new(self.module.functions[func].positions[ip - 1], message)
    }

    fn execute(&mut self, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
        let m = self.module;
        let mut func = 0usize;
        let mut code: &[Op] = &m.functions[0].code;
        let mut ip = 0usize;
        let mut base = 0usize;
        self.regs.resize(m.functions[0].n_regs as usize, Value::Null);

        macro_rules! reg {
            ($r:expr) => {
                self.regs[base + $r as usize]
            };
        }
        macro_rules! fail {
            ($msg:expr) => {
                return Err(self.error_at(func, ip, $msg))
            };
        }

        loop {
            let op = code[ip];
            ip += 1;
            match op {
                Op::LoadInt(d, n) => reg!(d) = Value::Int(n),
                Op::LoadReal(d, r) => reg!(d) = Value::Real(r),
                Op::LoadBool(d, b) => reg!(d) = Value::Bool(b),
                Op::LoadStr(d, i) => reg!(d) = Value::Str(m.strings[i as usize].clone()),
                Op::Move(d, s) => {
                    let v = reg!(s).clone();
                    reg!(d) = v;
                }
                Op::AddI(d, a, b) => match int(&reg!(a)).checked_add(int(&reg!(b))) {
                    Some(n) => reg!(d) = Value::Int(n),
                    None => fail!(OVERFLOW),
                },
                Op::SubI(d, a, b) => match int(&reg!(a)).checked_sub(int(&reg!(b))) {
                    Some(n) => reg!(d) = Value::Int(n),
                    None => fail!(OVERFLOW),
                },
                Op::MulI(d, a, b) => match int(&reg!(a)).checked_mul(int(&reg!(b))) {
                    Some(n) => reg!(d) = Value::Int(n),
                    None => fail!(OVERFLOW),
                },
                Op::DivI(d, a, b) => {
                    let (x, y) = (int(&reg!(a)), int(&reg!(b)));
                    if y == 0 {
                        fail!("division by zero");
                    }
                    match x.checked_div(y) {
                        Some(n) => reg!(d) = Value::Int(n),
                        None => fail!(OVERFLOW),
                    }
                }
                Op::ModI(d, a, b) => {
                    let (x, y) = (int(&reg!(a)), int(&reg!(b)));
                    if y == 0 {
                        fail!("division by zero");
                    }
                    match x.checked_rem(y) {
                        Some(n) => reg!(d) = Value::Int(n),
                        None => fail!(OVERFLOW),
                    }
                }
                Op::AddR(d, a, b) => reg!(d) = Value::Real(real(&reg!(a)) + real(&reg!(b))),
                Op::SubR(d, a, b) => reg!(d) = Value::Real(real(&reg!(a)) - real(&reg!(b))),
                Op::MulR(d, a, b) => reg!(d) = Value::Real(real(&reg!(a)) * real(&reg!(b))),
                Op::DivR(d, a, b) => {
                    let y = real(&reg!(b));
                    if y == 0.0 {
                        fail!("division by zero");
                    }
                    reg!(d) = Value::Real(real(&reg!(a)) / y);
                }
                Op::Concat(d, a, b) => {
                    let mut t = String::with_capacity(string(&reg!(a)).len() + string(&reg!(b)).len());
                    t.push_str(string(&reg!(a)));
                    t.push_str(string(&reg!(b)));
                    reg!(d) = Value::Str(t.into());
                }
                Op::NegI(d, s) => match int(&reg!(s)).checked_neg() {
                    Some(n) => reg!(d) = Value::Int(n),
                    None => fail!(OVERFLOW),
                },
                Op::NegR(d, s) => reg!(d) = Value::Real(-real(&reg!(s))),
                Op::Not(d, s) => reg!(d) = Value::Bool(!boolean(&reg!(s))),
                Op::ToReal(d, s) => reg!(d) = Value::Real(int(&reg!(s)) as f64),
                Op::ToStr(d, s) => {
                    let t = display(&reg!(s));
                    reg!(d) = Value::Str(t.into());
                }
                Op::CmpI(c, d, a, b) => {
                    let r = compare(c, &int(&reg!(a)), &int(&reg!(b)));
                    reg!(d) = Value::Bool(r);
                }
                Op::CmpR(c, d, a, b) => {
                    let r = compare(c, &real(&reg!(a)), &real(&reg!(b)));
                    reg!(d) = Value::Bool(r);
                }
                Op::CmpS(c, d, a, b) => {
                    let r = compare::<str>(c, string(&reg!(a)), string(&reg!(b)));
                    reg!(d) = Value::Bool(r);
                }
                Op::CmpB(c, d, a, b) => {
                    let r = compare(c, &boolean(&reg!(a)), &boolean(&reg!(b)));
                    reg!(d) = Value::Bool(r);
                }
                Op::Jump(t) => ip = t as usize,
                Op::JumpIfFalse(c, t) => {
                    if !boolean(&reg!(c)) {
                        ip = t as usize;
                    }
                }
                Op::JumpIfTrue(c, t) => {
                    if boolean(&reg!(c)) {
                        ip = t as usize;
                    }
                }
                Op::ForInit { var, end, step, exit } => {
                    let s = int(&reg!(step));
                    if s == 0 {
                        fail!("STEP cannot be 0");
                    }
                    let (v, e) = (int(&reg!(var)), int(&reg!(end)));
                    if (s > 0 && v > e) || (s < 0 && v < e) {
                        ip = exit as usize;
                    }
                }
                Op::ForStep { var, end, step, body } => {
                    let s = int(&reg!(step));
                    // on overflow the counter is past any end value: the loop is over
                    if let Some(v) = int(&reg!(var)).checked_add(s) {
                        let e = int(&reg!(end));
                        reg!(var) = Value::Int(v);
                        if (s > 0 && v <= e) || (s < 0 && v >= e) {
                            ip = body as usize;
                        }
                    }
                }
                Op::Print(s) => {
                    let t = display(&reg!(s));
                    let _ = output.write_all(t.as_bytes());
                }
                Op::PrintLn => {
                    let _ = output.write_all(b"\n");
                }
                Op::RetNone => {
                    let Some(fr) = self.frames.pop() else { return Ok(()) };
                    self.regs.truncate(base);
                    func = fr.func;
                    code = &m.functions[func].code;
                    ip = fr.ip;
                    base = fr.base;
                    let _ = fr.dst;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{Cmp, Function, Module, Op::*};
    use crate::errors::Pos;

    /// Runs one main function. Instruction k reports errors at line k + 1.
    pub(crate) fn run_code(code: Vec<Op>, n_regs: u16, strings: &[&str], input: &str) -> Result<String, Error> {
        let positions = (0..code.len()).map(|k| Pos::new(k as u32 + 1, 0, 0)).collect();
        let module = Module {
            functions: vec![Function { name: "main".into(), n_params: 0, n_regs, code, positions }],
            strings: strings.iter().map(|s| Rc::from(*s)).collect(),
            globals: vec![],
        };
        let mut out = Vec::new();
        run(&module, &mut input.as_bytes(), &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn integer_and_real_arithmetic() {
        let code = vec![
            LoadInt(0, 7), LoadInt(1, 2),
            AddI(2, 0, 1), Print(2), PrintLn,
            DivI(2, 0, 1), Print(2), PrintLn,
            ModI(2, 0, 1), Print(2), PrintLn,
            ToReal(3, 0), ToReal(4, 1), DivR(5, 3, 4), Print(5), PrintLn,
            LoadReal(5, 3.0), Print(5), PrintLn,
            NegI(2, 0), Print(2), PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 6, &[], "").unwrap(), "9\n3\n1\n3.5\n3.0\n-7\n");
    }

    #[test]
    fn negative_div_and_mod_truncate() {
        let code = vec![LoadInt(0, -7), LoadInt(1, 2), DivI(2, 0, 1), ModI(3, 0, 1), Print(2), Print(3), PrintLn, RetNone];
        assert_eq!(run_code(code, 4, &[], "").unwrap(), "-3-1\n");
    }

    #[test]
    fn overflow_is_an_error() {
        let code = vec![LoadInt(0, i64::MAX), LoadInt(1, 1), AddI(2, 0, 1), RetNone];
        let e = run_code(code, 3, &[], "").unwrap_err();
        assert_eq!(e.message, "number too large: the result exceeds the integer limits");
        assert_eq!(e.pos.line, 3);
    }

    #[test]
    fn division_by_zero_is_an_error() {
        let code = vec![LoadInt(0, 1), LoadInt(1, 0), DivI(2, 0, 1), RetNone];
        assert_eq!(run_code(code, 3, &[], "").unwrap_err().message, "division by zero");
        let code = vec![LoadReal(0, 1.0), LoadReal(1, 0.0), DivR(2, 0, 1), RetNone];
        assert_eq!(run_code(code, 3, &[], "").unwrap_err().message, "division by zero");
    }

    #[test]
    fn while_style_loop() {
        let code = vec![
            LoadInt(0, 0),            // 0: s
            LoadInt(1, 1),            // 1: i
            LoadInt(2, 5),            // 2: limit
            LoadInt(3, 1),            // 3: one
            CmpI(Cmp::LtEq, 4, 1, 2), // 4
            JumpIfFalse(4, 9),        // 5
            AddI(0, 0, 1),            // 6
            AddI(1, 1, 3),            // 7
            Jump(4),                  // 8
            Print(0),                 // 9
            PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 5, &[], "").unwrap(), "15\n");
    }

    #[test]
    fn for_loop_with_negative_step() {
        let code = vec![
            LoadInt(0, 10), LoadInt(1, 0), LoadInt(2, -2),
            ForInit { var: 0, end: 1, step: 2, exit: 7 },
            Print(0), PrintLn,
            ForStep { var: 0, end: 1, step: 2, body: 4 },
            RetNone,
        ];
        assert_eq!(run_code(code, 3, &[], "").unwrap(), "10\n8\n6\n4\n2\n0\n");
    }

    #[test]
    fn for_loop_that_never_runs_and_zero_step() {
        let skip = vec![
            LoadInt(0, 5), LoadInt(1, 1), LoadInt(2, 1),
            ForInit { var: 0, end: 1, step: 2, exit: 6 },
            Print(0),
            ForStep { var: 0, end: 1, step: 2, body: 4 },
            RetNone,
        ];
        assert_eq!(run_code(skip, 3, &[], "").unwrap(), "");
        let zero = vec![LoadInt(0, 1), LoadInt(1, 5), LoadInt(2, 0), ForInit { var: 0, end: 1, step: 2, exit: 4 }, RetNone];
        assert_eq!(run_code(zero, 3, &[], "").unwrap_err().message, "STEP cannot be 0");
    }

    #[test]
    fn strings_booleans_and_comparisons() {
        let code = vec![
            LoadStr(0, 0), LoadReal(1, 7.5), ToStr(2, 1), Concat(3, 0, 2), Print(3), PrintLn,
            LoadStr(4, 1), CmpS(Cmp::Lt, 5, 0, 4), Print(5), PrintLn,
            Not(5, 5), Print(5), PrintLn,
            LoadBool(6, true), CmpB(Cmp::Eq, 7, 5, 6), Print(7), PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 8, &["Grade: ", "Z"], "").unwrap(), "Grade: 7.5\nTRUE\nFALSE\nFALSE\n");
    }

    #[test]
    fn real_formatting() {
        assert_eq!(format_real(3.0), "3.0");
        assert_eq!(format_real(2.5), "2.5");
        assert_eq!(format_real(-0.25), "-0.25");
        assert_eq!(format_real(0.1 + 0.2), "0.30000000000000004");
    }
}
