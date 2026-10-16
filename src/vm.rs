//! The virtual machine: executes the register bytecode produced by the compiler.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::rc::Rc;

use crate::builtins::{Builtin, Env};
use crate::bytecode::{Cmp, Module, Op};
use crate::errors::Error;
use crate::tir::BaseType;

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
    Dict(Rc<RefCell<Dict>>),
    Object(Rc<RefCell<Obj>>),
}

/// An object: its class id and the values of its fields, in slot order.
#[derive(Debug)]
pub struct Obj {
    pub class: u16,
    pub fields: Vec<Value>,
}

/// A dictionary key: dictionaries are keyed by ints or strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Int(i64),
    Str(Rc<str>),
}

impl Key {
    pub fn of(v: &Value) -> Key {
        match v {
            Value::Int(n) => Key::Int(*n),
            Value::Str(s) => Key::Str(s.clone()),
            other => panic!("internal error: {:?} is not a dictionary key", other),
        }
    }

    pub fn value(&self) -> Value {
        match self {
            Key::Int(n) => Value::Int(*n),
            Key::Str(s) => Value::Str(s.clone()),
        }
    }

    /// The key as written in a program, for error messages.
    pub fn shown(&self) -> String {
        match self {
            Key::Int(n) => n.to_string(),
            Key::Str(s) => format!("\"{}\"", s),
        }
    }
}

/// A dictionary that remembers the order in which keys were first added.
#[derive(Debug, Default)]
pub struct Dict {
    entries: Vec<(Key, Value)>,
    index: std::collections::HashMap<Key, usize>,
}

impl Dict {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, k: &Key) -> Option<&Value> {
        self.index.get(k).map(|&i| &self.entries[i].1)
    }

    pub fn set(&mut self, k: Key, v: Value) {
        match self.index.get(&k) {
            Some(&i) => self.entries[i].1 = v,
            None => {
                self.index.insert(k.clone(), self.entries.len());
                self.entries.push((k, v));
            }
        }
    }

    /// Removes a key; false if it was not there.
    pub fn remove(&mut self, k: &Key) -> bool {
        let Some(i) = self.index.remove(k) else { return false };
        self.entries.remove(i);
        for (_, j) in self.index.iter_mut() {
            if *j > i {
                *j -= 1;
            }
        }
        true
    }

    pub fn keys(&self) -> impl Iterator<Item = &Key> {
        self.entries.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().map(|(_, v)| v)
    }
}

pub fn missing_key(k: &Key) -> String {
    format!("key {} is not in the dictionary", k.shown())
}

#[derive(Debug)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Value>,
}

const OVERFLOW: &str = "number too large: the result exceeds the integer limits";
const MAX_CALL_DEPTH: usize = 100_000;

fn seq(v: &Value) -> &Rc<RefCell<Vec<Value>>> {
    match v {
        Value::Seq(s) => s,
        other => panic!("internal error: expected an array or list, found {:?}", other),
    }
}

fn dict(v: &Value) -> &Rc<RefCell<Dict>> {
    match v {
        Value::Dict(d) => d,
        other => panic!("internal error: expected a dictionary, found {:?}", other),
    }
}

fn object(v: &Value) -> &Rc<RefCell<Obj>> {
    match v {
        Value::Object(o) => o,
        other => panic!("internal error: expected an object, found {:?}", other),
    }
}

fn matrix(v: &Value) -> &Rc<RefCell<Matrix>> {
    match v {
        Value::Matrix(m) => m,
        other => panic!("internal error: expected a matrix, found {:?}", other),
    }
}

/// 1-based index → 0-based position, or the error message.
fn position(i: i64, len: usize) -> Result<usize, String> {
    if i >= 1 && (i as u64) <= len as u64 {
        Ok(i as usize - 1)
    } else if len == 0 {
        Err(format!("index {} is out of range: the list is empty", i))
    } else {
        Err(format!("index {} is out of range: valid positions are 1 to {}", i, len))
    }
}

fn cell(i: i64, j: i64, m: &Matrix) -> Result<usize, String> {
    if i < 1 || i as u64 > m.rows as u64 {
        return Err(format!("row {} is out of range: valid rows are 1 to {}", i, m.rows));
    }
    if j < 1 || j as u64 > m.cols as u64 {
        return Err(format!("column {} is out of range: valid columns are 1 to {}", j, m.cols));
    }
    Ok((i as usize - 1) * m.cols + (j as usize - 1))
}

fn parse_input(text: &str, ty: BaseType) -> Result<Value, String> {
    let t = text.trim();
    match ty {
        BaseType::Int => t.parse().map(Value::Int).map_err(|_| format!("expected an int, read \"{}\"", text)),
        BaseType::Real => match t.replace(',', ".").parse::<f64>() {
            Ok(r) if r.is_finite() => Ok(Value::Real(r)),
            _ => Err(format!("expected a float (like 3.5), read \"{}\"", text)),
        },
        BaseType::Bool => match t.to_lowercase().as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(format!("expected true or false, read \"{}\"", text)),
        },
        BaseType::Str => Ok(Value::Str(text.into())),
    }
}

/// Shortest representation of a real, always with a decimal point.
pub fn format_real(r: f64) -> String {
    let s = r.to_string();
    if r.is_finite() && !s.contains('.') {
        s + ".0"
    } else {
        s
    }
}

pub(crate) fn display(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        other => repr(other),
    }
}

/// A value as Python writes it inside a collection: strings in quotes,
/// lists in [ ], dictionaries in { }. An element with no value yet is "?".
fn repr(v: &Value) -> String {
    repr_at(v, 0)
}

/// Like `repr`, but stops at a fixed depth so cyclic structures cannot loop.
fn repr_at(v: &Value, depth: usize) -> String {
    if depth > 12 {
        return "...".to_string();
    }
    let items =
        |values: &mut dyn Iterator<Item = &Value>| values.map(|x| repr_at(x, depth + 1)).collect::<Vec<_>>().join(", ");
    match v {
        Value::Null => "?".to_string(),
        Value::Int(n) => n.to_string(),
        Value::Real(r) => format_real(*r),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Str(s) if s.contains('\'') && !s.contains('"') => format!("\"{}\"", s),
        Value::Str(s) => format!("'{}'", s.replace('\'', "\\'")),
        Value::Seq(q) => format!("[{}]", items(&mut q.borrow().iter())),
        Value::Matrix(m) => {
            let m = m.borrow();
            let rows: Vec<String> =
                m.data.chunks(m.cols.max(1)).map(|row| format!("[{}]", items(&mut row.iter()))).collect();
            format!("[{}]", rows.join(", "))
        }
        Value::Dict(d) => {
            let d = d.borrow();
            let pairs: Vec<String> = d
                .keys()
                .zip(d.values())
                .map(|(k, v)| format!("{}: {}", repr_at(&k.value(), depth + 1), repr_at(v, depth + 1)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        Value::Object(o) => {
            let o = o.borrow();
            let names = crate::tir::class_field_names(o.class);
            let pairs: Vec<String> =
                names.iter().zip(&o.fields).map(|(n, f)| format!("{}={}", n, repr_at(f, depth + 1))).collect();
            format!("{}({})", crate::tir::class_name(o.class), pairs.join(", "))
        }
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
        env: Env::default(),
        prof: None,
    };
    let result = vm.execute(input, output);
    let _ = output.flush();
    if result.is_ok() {
        vm.env.finish();
    }
    result
}

/// Runs the program counting how often each opcode executes, for the
/// `--profile` flag. Returns the counts, most frequent first.
pub fn run_profiled(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<Vec<(&'static str, u64)>, Error> {
    let mut vm = Vm {
        module,
        regs: Vec::with_capacity(1 << 12),
        globals: vec![Value::Null; module.globals.len()],
        frames: Vec::new(),
        env: Env::default(),
        prof: Some(HashMap::new()),
    };
    let result = vm.execute(input, output);
    let _ = output.flush();
    if result.is_ok() {
        vm.env.finish();
    }
    result?;
    let mut counts: Vec<(&'static str, u64)> = vm.prof.unwrap().into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(counts)
}

struct Vm<'m> {
    module: &'m Module,
    regs: Vec<Value>,
    globals: Vec<Value>,
    frames: Vec<Frame>,
    env: Env,
    /// when profiling: how many times each opcode ran
    prof: Option<HashMap<&'static str, u64>>,
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
            if let Some(p) = self.prof.as_mut() {
                *p.entry(op.name()).or_insert(0) += 1;
            }
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
                Op::JumpUnlessI(c, a, b, t) => {
                    if !compare(c, &int(&reg!(a)), &int(&reg!(b))) {
                        ip = t as usize;
                    }
                }
                Op::JumpUnlessIImm(c, a, n, t) => {
                    if !compare(c, &int(&reg!(a)), &n) {
                        ip = t as usize;
                    }
                }
                Op::CmpIImm(c, d, a, n) => reg!(d) = Value::Bool(compare(c, &int(&reg!(a)), &n)),
                Op::AddIImm(d, a, n) => match int(&reg!(a)).checked_add(n) {
                    Some(v) => reg!(d) = Value::Int(v),
                    None => fail!(OVERFLOW),
                },
                Op::SubIImm(d, a, n) => match int(&reg!(a)).checked_sub(n) {
                    Some(v) => reg!(d) = Value::Int(v),
                    None => fail!(OVERFLOW),
                },
                Op::MulIImm(d, a, n) => match int(&reg!(a)).checked_mul(n) {
                    Some(v) => reg!(d) = Value::Int(v),
                    None => fail!(OVERFLOW),
                },
                Op::DivIImm(d, a, n) => match int(&reg!(a)).checked_div(n) {
                    Some(v) => reg!(d) = Value::Int(v),
                    None => fail!(OVERFLOW),
                },
                Op::ModIImm(d, a, n) => match int(&reg!(a)).checked_rem(n) {
                    Some(v) => reg!(d) = Value::Int(v),
                    None => fail!(OVERFLOW),
                },
                Op::ForInit { var, end, step, exit } => {
                    let s = int(&reg!(step));
                    if s == 0 {
                        fail!("step cannot be 0");
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
                Op::GetGlobal(d, g) => {
                    let v = self.globals[g as usize].clone();
                    if let Value::Null = v {
                        fail!(format!("variable \"{}\" has no value yet", m.globals[g as usize]));
                    }
                    reg!(d) = v;
                }
                Op::SetGlobal(g, s) => {
                    let v = reg!(s).clone();
                    self.globals[g as usize] = v;
                }
                Op::NewArray(d, n) => reg!(d) = Value::Seq(Rc::new(RefCell::new(vec![Value::Null; n as usize]))),
                Op::NewMatrix(d, r, c) => {
                    let (rows, cols) = (r as usize, c as usize);
                    let data = vec![Value::Null; rows * cols];
                    reg!(d) = Value::Matrix(Rc::new(RefCell::new(Matrix { rows, cols, data })));
                }
                Op::NewList(d) => reg!(d) = Value::Seq(Rc::new(RefCell::new(Vec::new()))),
                Op::NewDict(d) => reg!(d) = Value::Dict(Rc::new(RefCell::new(Dict::default()))),
                Op::NewObject(d, class, _n) => {
                    let fields = m.class_fields[class as usize]
                        .iter()
                        .map(|&kind| match kind {
                            1 => Value::Seq(Rc::new(RefCell::new(Vec::new()))),
                            2 => Value::Dict(Rc::new(RefCell::new(Dict::default()))),
                            3 => Value::Int(0),
                            4 => Value::Real(0.0),
                            5 => Value::Bool(false),
                            6 => Value::Str("".into()),
                            _ => Value::Null,
                        })
                        .collect();
                    reg!(d) = Value::Object(Rc::new(RefCell::new(Obj { class: class as u16, fields })));
                }
                Op::GetField(d, o, slot) => {
                    let v = object(&reg!(o)).borrow().fields[slot as usize].clone();
                    if let Value::Null = v {
                        fail!("this field has no value yet");
                    }
                    reg!(d) = v;
                }
                Op::SetField(o, slot, s) => {
                    let v = reg!(s).clone();
                    object(&reg!(o)).borrow_mut().fields[slot as usize] = v;
                }
                Op::GetKey(d, c, i) => {
                    let k = Key::of(&reg!(i));
                    let v = dict(&reg!(c)).borrow().get(&k).cloned();
                    match v {
                        Some(v) => reg!(d) = v,
                        None => fail!(missing_key(&k)),
                    }
                }
                Op::SetKey(c, i, s) => {
                    let (k, v) = (Key::of(&reg!(i)), reg!(s).clone());
                    dict(&reg!(c)).borrow_mut().set(k, v);
                }
                Op::GetElem(d, c, i) => {
                    let i = int(&reg!(i));
                    let v = {
                        let s = seq(&reg!(c)).borrow();
                        match position(i, s.len()) {
                            Ok(k) => s[k].clone(),
                            Err(msg) => fail!(msg),
                        }
                    };
                    if let Value::Null = v {
                        fail!("this element has no value yet");
                    }
                    reg!(d) = v;
                }
                Op::SetElem(c, i, s) => {
                    let i = int(&reg!(i));
                    let v = reg!(s).clone();
                    let mut q = seq(&reg!(c)).borrow_mut();
                    match position(i, q.len()) {
                        Ok(k) => q[k] = v,
                        Err(msg) => fail!(msg),
                    }
                }
                Op::GetElem2(d, c, i, j) => {
                    let (i, j) = (int(&reg!(i)), int(&reg!(j)));
                    let v = {
                        let mx = matrix(&reg!(c)).borrow();
                        match cell(i, j, &mx) {
                            Ok(k) => mx.data[k].clone(),
                            Err(msg) => fail!(msg),
                        }
                    };
                    if let Value::Null = v {
                        fail!("this element has no value yet");
                    }
                    reg!(d) = v;
                }
                Op::SetElem2(c, i, j, s) => {
                    let (i, j) = (int(&reg!(i)), int(&reg!(j)));
                    let v = reg!(s).clone();
                    let mut mx = matrix(&reg!(c)).borrow_mut();
                    match cell(i, j, &mx) {
                        Ok(k) => mx.data[k] = v,
                        Err(msg) => fail!(msg),
                    }
                }
                Op::CharAt(d, s, i) => {
                    let i = int(&reg!(i));
                    let ch = {
                        let text = string(&reg!(s));
                        let n = text.chars().count();
                        match position(i, n) {
                            Ok(k) => text.chars().nth(k).expect("index checked").to_string(),
                            Err(_) => fail!(format!("index {} is out of range: the string has {} characters", i, n)),
                        }
                    };
                    reg!(d) = Value::Str(ch.into());
                }
                Op::Length(d, s) => {
                    let n = match &reg!(s) {
                        Value::Seq(q) => q.borrow().len(),
                        Value::Dict(t) => t.borrow().len(),
                        Value::Str(t) => t.chars().count(),
                        other => panic!("internal error: length of {:?}", other),
                    };
                    reg!(d) = Value::Int(n as i64);
                }
                Op::Append(l, s) => {
                    let v = reg!(s).clone();
                    seq(&reg!(l)).borrow_mut().push(v);
                }
                Op::Remove(l, i) => {
                    let i = int(&reg!(i));
                    let mut q = seq(&reg!(l)).borrow_mut();
                    match position(i, q.len()) {
                        Ok(k) => {
                            q.remove(k);
                        }
                        Err(msg) => fail!(msg),
                    }
                }
                Op::Call { func: f, first, argc, dst } => {
                    if self.frames.len() >= MAX_CALL_DEPTH {
                        fail!("too many nested calls (maybe a recursion that never ends)");
                    }
                    let callee = &m.functions[f as usize];
                    let new_base = self.regs.len();
                    let n_regs = callee.n_regs as usize;
                    // push the arguments straight into the callee's registers, then
                    // fill the remaining locals with "no value yet" in one pass
                    let start = base + first as usize;
                    self.regs.reserve(n_regs);
                    for k in 0..argc as usize {
                        let v = self.regs[start + k].clone();
                        self.regs.push(v);
                    }
                    for _ in argc as usize..n_regs {
                        self.regs.push(Value::Null);
                    }
                    self.frames.push(Frame { func, ip, base, dst });
                    func = f as usize;
                    code = &callee.code;
                    ip = 0;
                    base = new_base;
                }
                Op::Ensure { flag, func: f } => {
                    if !matches!(self.globals[flag as usize], Value::Bool(true)) {
                        self.globals[flag as usize] = Value::Bool(true);
                        if self.frames.len() >= MAX_CALL_DEPTH {
                            fail!("too many nested calls (maybe a recursion that never ends)");
                        }
                        let callee = &m.functions[f as usize];
                        let new_base = self.regs.len();
                        self.regs.resize(new_base + callee.n_regs as usize, Value::Null);
                        self.frames.push(Frame { func, ip, base, dst: 0 });
                        func = f as usize;
                        code = &callee.code;
                        ip = 0;
                        base = new_base;
                    }
                }
                Op::Ret(s) => {
                    let v = std::mem::replace(&mut reg!(s), Value::Null);
                    let Some(fr) = self.frames.pop() else { return Ok(()) };
                    self.regs.truncate(base);
                    func = fr.func;
                    code = &m.functions[func].code;
                    ip = fr.ip;
                    base = fr.base;
                    reg!(fr.dst) = v;
                }
                Op::RetNone => {
                    let Some(fr) = self.frames.pop() else { return Ok(()) };
                    self.regs.truncate(base);
                    func = fr.func;
                    code = &m.functions[func].code;
                    ip = fr.ip;
                    base = fr.base;
                }
                Op::Builtin { id, first, argc, dst } => {
                    let b = Builtin::from_id(id);
                    if matches!(b, Builtin::Message | Builtin::Confirm | Builtin::Ask) {
                        let _ = output.flush();
                    }
                    let start = base + first as usize;
                    match self.env.call(b, &self.regs[start..start + argc as usize]) {
                        Ok(v) => {
                            if b == Builtin::Stop {
                                self.env.close_window();
                                return Ok(());
                            }
                            if b.is_function() {
                                reg!(dst) = v;
                            }
                        }
                        Err(msg) => fail!(msg),
                    }
                }
                Op::Read(d, ty) => {
                    let _ = output.flush();
                    let mut line = String::new();
                    match input.read_line(&mut line) {
                        Ok(0) | Err(_) => fail!("the input is over: the program expected another value"),
                        Ok(_) => {}
                    }
                    let text = line.trim_end_matches(|c| c == '\n' || c == '\r');
                    match parse_input(text, ty) {
                        Ok(v) => reg!(d) = v,
                        Err(msg) => fail!(msg),
                    }
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
            class_fields: vec![],
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
        assert_eq!(run_code(zero, 3, &[], "").unwrap_err().message, "step cannot be 0");
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
        assert_eq!(run_code(code, 8, &["Grade: ", "Z"], "").unwrap(), "Grade: 7.5\ntrue\nfalse\nfalse\n");
    }

    #[test]
    fn real_formatting() {
        assert_eq!(format_real(3.0), "3.0");
        assert_eq!(format_real(2.5), "2.5");
        assert_eq!(format_real(-0.25), "-0.25");
        assert_eq!(format_real(0.1 + 0.2), "0.30000000000000004");
    }

    use crate::tir::BaseType;

    #[test]
    fn arrays_and_lengths() {
        let code = vec![
            NewArray(0, 3), LoadInt(1, 2), LoadInt(2, 42),
            SetElem(0, 1, 2), GetElem(3, 0, 1), Print(3), PrintLn,
            Length(4, 0), Print(4), PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 5, &[], "").unwrap(), "42\n3\n");
    }

    #[test]
    fn element_errors() {
        let out_of_range = vec![NewArray(0, 3), LoadInt(1, 4), GetElem(2, 0, 1), RetNone];
        let e = run_code(out_of_range, 3, &[], "").unwrap_err();
        assert_eq!(e.message, "index 4 is out of range: valid positions are 1 to 3");
        assert_eq!(e.pos.line, 3);
        let unset = vec![NewArray(0, 3), LoadInt(1, 1), GetElem(2, 0, 1), RetNone];
        assert_eq!(run_code(unset, 3, &[], "").unwrap_err().message, "this element has no value yet");
    }

    #[test]
    fn matrices() {
        let code = vec![
            NewMatrix(0, 2, 3), LoadInt(1, 2), LoadInt(2, 3), LoadInt(3, 9),
            SetElem2(0, 1, 2, 3), GetElem2(4, 0, 1, 2), Print(4), PrintLn,
            LoadInt(1, 3), GetElem2(4, 0, 1, 2),
            RetNone,
        ];
        let e = run_code(code, 5, &[], "").unwrap_err();
        assert_eq!(e.message, "row 3 is out of range: valid rows are 1 to 2");
    }

    #[test]
    fn lists() {
        let code = vec![
            NewList(0), LoadStr(1, 0), Append(0, 1), LoadStr(1, 1), Append(0, 1), LoadStr(1, 2), Append(0, 1),
            LoadInt(2, 2), Remove(0, 2),
            Length(3, 0), Print(3), PrintLn,
            GetElem(4, 0, 2), Print(4), PrintLn,
            LoadInt(2, 9), Remove(0, 2),
            RetNone,
        ];
        let e = run_code(code, 5, &["a", "b", "c"], "").unwrap_err();
        assert_eq!(e.message, "index 9 is out of range: valid positions are 1 to 2");
        let empty = vec![NewList(0), LoadInt(1, 1), GetElem(2, 0, 1), RetNone];
        assert_eq!(run_code(empty, 3, &[], "").unwrap_err().message, "index 1 is out of range: the list is empty");
    }

    #[test]
    fn list_output_before_error() {
        let code = vec![
            NewList(0), LoadStr(1, 0), Append(0, 1), Length(2, 0), Print(2), PrintLn, RetNone,
        ];
        assert_eq!(run_code(code, 3, &["x"], "").unwrap(), "1\n");
    }

    #[test]
    fn characters_of_strings() {
        let code = vec![LoadStr(0, 0), LoadInt(1, 2), CharAt(2, 0, 1), Print(2), Length(3, 0), Print(3), PrintLn, RetNone];
        assert_eq!(run_code(code, 4, &["città"], "").unwrap(), "i5\n");
    }

    fn two_functions(main: Vec<Op>, main_regs: u16, f: Vec<Op>, f_regs: u16) -> Module {
        let pos = |n: usize| (0..n).map(|k| Pos::new(k as u32 + 1, 0, 0)).collect();
        Module {
            functions: vec![
                Function { name: "main".into(), n_params: 0, n_regs: main_regs, positions: pos(main.len()), code: main },
                Function { name: "f".into(), n_params: 1, n_regs: f_regs, positions: pos(f.len()), code: f },
            ],
            strings: vec![],
            globals: vec!["g".into()],
            class_fields: vec![],
        }
    }

    fn run_module(m: &Module, input: &str) -> Result<String, Error> {
        let mut out = Vec::new();
        run(m, &mut input.as_bytes(), &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn calls_and_returns() {
        // f(x) = x * 2
        let f = vec![LoadInt(1, 2), MulI(1, 0, 1), Ret(1)];
        let main = vec![LoadInt(1, 21), Call { func: 1, first: 1, argc: 1, dst: 0 }, Print(0), PrintLn, RetNone];
        assert_eq!(run_module(&two_functions(main, 2, f, 2), "").unwrap(), "42\n");
    }

    #[test]
    fn runaway_recursion_is_an_error() {
        let f = vec![Call { func: 1, first: 0, argc: 1, dst: 0 }, RetNone];
        let main = vec![LoadInt(0, 1), Call { func: 1, first: 0, argc: 1, dst: 0 }, RetNone];
        let e = run_module(&two_functions(main, 1, f, 1), "").unwrap_err();
        assert_eq!(e.message, "too many nested calls (maybe a recursion that never ends)");
    }

    #[test]
    fn globals() {
        let main = vec![LoadInt(0, 5), SetGlobal(0, 0), GetGlobal(1, 0), Print(1), PrintLn, RetNone];
        assert_eq!(run_module(&two_functions(main, 2, vec![RetNone], 1), "").unwrap(), "5\n");
        let unset = vec![GetGlobal(0, 0), RetNone];
        let e = run_module(&two_functions(unset, 1, vec![RetNone], 1), "").unwrap_err();
        assert_eq!(e.message, "variable \"g\" has no value yet");
    }

    #[test]
    fn read_converts_and_trims() {
        let code = vec![
            Read(0, BaseType::Int), Read(1, BaseType::Real), Read(2, BaseType::Str), Read(3, BaseType::Bool),
            Print(0), Print(1), Print(2), Print(3), PrintLn,
            RetNone,
        ];
        assert_eq!(run_code(code, 4, &[], "  42 \r\n3,5\nhello world\r\ntrue\n").unwrap(), "423.5hello worldtrue\n");
        let bad = vec![Read(0, BaseType::Int), RetNone];
        assert_eq!(run_code(bad.clone(), 1, &[], "abc\n").unwrap_err().message, "expected an int, read \"abc\"");
        assert_eq!(
            run_code(bad, 1, &[], "").unwrap_err().message,
            "the input is over: the program expected another value"
        );
    }

    #[test]
    fn float_input_must_be_a_real_number() {
        for text in ["nan\n", "inf\n", "1e400\n"] {
            let e = run_code(vec![Read(0, BaseType::Real), RetNone], 1, &[], text).unwrap_err();
            assert!(e.message.starts_with("expected a float (like 3.5)"), "{}: {}", text, e.message);
        }
    }
}
