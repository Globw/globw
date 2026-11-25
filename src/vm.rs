//! The virtual machine: executes the register bytecode produced by the compiler.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
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
    /// a compact array of ints, floats or bools (`int[n]`, `float[n]`, `bool[n]`)
    Packed(Rc<RefCell<Packed>>),
    Matrix(Rc<RefCell<Matrix>>),
    Dict(Rc<RefCell<Dict>>),
    Object(Rc<RefCell<Obj>>),
    /// a function value: the index of a compiled function
    Func(u32),
    /// a set of simple values, kept as the keys of a dictionary
    Set(Rc<RefCell<Dict>>),
    /// a grid: a contiguous array of floats (the `nungw` engine), plus its
    /// shape (the sizes along each dimension; `[n]` for a 1-D array). The data
    /// is row-major. The shape is a cheap `Rc` so cloning the value is cheap.
    Floats(Rc<RefCell<Vec<f64>>>, Rc<Vec<usize>>),
    /// a `nungw` array resident in GPU memory (the `gpu` module holds the buffer;
    /// this keeps a handle that frees it on drop, plus the shape). Data is f32.
    #[cfg(feature = "gpu")]
    GpuFloats(Rc<crate::gpu::GpuHandle>, Rc<Vec<usize>>),
}

/// A compact array: one machine value per element, and for ints and floats one bit
/// per element saying it has a value (reading one that has none is an error, as
/// with any array); no bits at all (an empty vector) once every element has one.
/// Bools keep 0 (false), 1 (true) or 2 (no value) in a byte.
#[derive(Debug, Clone)]
pub enum Packed {
    Ints(Vec<i64>, Vec<u64>),
    Reals(Vec<f64>, Vec<u64>),
    Bools(Vec<u8>),
}

pub const UNSET_BOOL: u8 = 2;

/// "Has a value" bits for `n` elements that all have one.
fn full_bits(n: usize) -> Vec<u64> {
    let mut b = vec![u64::MAX; n.div_ceil(64)];
    if n % 64 != 0 {
        if let Some(last) = b.last_mut() {
            *last = (1u64 << (n % 64)) - 1;
        }
    }
    b
}

impl Packed {
    pub fn new(n: usize, kind: u8) -> Packed {
        let bits = vec![0u64; n.div_ceil(64)];
        match kind {
            0 => Packed::Ints(vec![0; n], bits),
            1 => Packed::Reals(vec![0.0; n], bits),
            _ => Packed::Bools(vec![UNSET_BOOL; n]),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Packed::Ints(v, _) => v.len(),
            Packed::Reals(v, _) => v.len(),
            Packed::Bools(v) => v.len(),
        }
    }

    /// Element `k` (0-based, in range); `Value::Null` when it has no value yet.
    pub fn get(&self, k: usize) -> Value {
        let has = |bits: &Vec<u64>| bits.is_empty() || bits[k >> 6] >> (k & 63) & 1 == 1;
        match self {
            Packed::Ints(v, b) if has(b) => Value::Int(v[k]),
            Packed::Reals(v, b) if has(b) => Value::Real(v[k]),
            Packed::Bools(v) if v[k] != UNSET_BOOL => Value::Bool(v[k] == 1),
            _ => Value::Null,
        }
    }

    /// Stores `x` at `k` (0-based, in range). `Null` takes the value away; an int
    /// stored in a float array becomes a float.
    pub fn set(&mut self, k: usize, x: &Value) {
        let mark = |b: &mut Vec<u64>| {
            if !b.is_empty() {
                b[k >> 6] |= 1 << (k & 63);
            }
        };
        match (self, x) {
            (Packed::Ints(v, b), Value::Int(n)) => {
                v[k] = *n;
                mark(b);
            }
            (Packed::Reals(v, b), Value::Real(r)) => {
                v[k] = *r;
                mark(b);
            }
            (Packed::Reals(v, b), Value::Int(n)) => {
                v[k] = *n as f64;
                mark(b);
            }
            (Packed::Bools(v), Value::Bool(t)) => v[k] = *t as u8,
            (Packed::Ints(v, b), _) => {
                if b.is_empty() {
                    *b = full_bits(v.len());
                }
                b[k >> 6] &= !(1 << (k & 63));
            }
            (Packed::Reals(v, b), _) => {
                if b.is_empty() {
                    *b = full_bits(v.len());
                }
                b[k >> 6] &= !(1 << (k & 63));
            }
            (Packed::Bools(v), _) => v[k] = UNSET_BOOL,
        }
    }

    pub fn push(&mut self, x: &Value) {
        let k = self.len();
        let number = matches!(x, Value::Int(_) | Value::Real(_));
        match self {
            Packed::Ints(v, b) => {
                v.push(0);
                if b.is_empty() && !number {
                    *b = full_bits(k);
                }
                if !b.is_empty() && b.len() * 64 <= k {
                    b.push(0);
                }
            }
            Packed::Reals(v, b) => {
                v.push(0.0);
                if b.is_empty() && !number {
                    *b = full_bits(k);
                }
                if !b.is_empty() && b.len() * 64 <= k {
                    b.push(0);
                }
            }
            Packed::Bools(v) => v.push(UNSET_BOOL),
        }
        self.set(k, x);
    }

    /// Drops the "has a value" bits when every element has one (cheap when not:
    /// it stops at the first gap, looking at both ends first). True when full.
    pub fn try_full(&mut self) -> bool {
        let (n, b) = match self {
            Packed::Ints(v, b) => (v.len(), b),
            Packed::Reals(v, b) => (v.len(), b),
            Packed::Bools(_) => return false,
        };
        if b.is_empty() {
            return true;
        }
        let words = n / 64;
        let tail = n % 64;
        let whole = |w: &u64| *w == u64::MAX;
        if words > 0 && (!whole(&b[0]) || !whole(&b[words - 1])) {
            return false;
        }
        if tail > 0 && b[words] != (1u64 << tail) - 1 {
            return false;
        }
        if !b[..words].iter().all(whole) {
            return false;
        }
        *b = Vec::new();
        true
    }

    /// The elements as values (`Null` where there is none), for code that works on
    /// any list.
    pub fn values(&self) -> Vec<Value> {
        (0..self.len()).map(|k| self.get(k)).collect()
    }

    /// Takes back elements from a list made with `values` (after a library call
    /// that may have changed it, e.g. a sort).
    pub fn take(&mut self, items: &[Value]) {
        if items.len() == self.len() {
            for (k, x) in items.iter().enumerate() {
                self.set(k, x);
            }
        }
    }
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
const MAX_TASKS: usize = 100_000;

/// A suspended cooperative task: its own registers, call stack and handlers, plus
/// where it was running (a green thread, not an OS thread).
struct TaskSlot {
    regs: Vec<Value>,
    frames: Vec<Frame>,
    handlers: Vec<Handler>,
    func: usize,
    ip: usize,
    base: usize,
    done: bool,
    /// handles of tasks blocked in task.await on this one, woken when it finishes
    waiters: Vec<usize>,
}

impl TaskSlot {
    fn empty() -> TaskSlot {
        TaskSlot {
            regs: Vec::new(),
            frames: Vec::new(),
            handlers: Vec::new(),
            func: 0,
            ip: 0,
            base: 0,
            done: false,
            waiters: Vec::new(),
        }
    }
}

/// A FIFO channel between tasks: it carries text values; a `task.recv` on an empty
/// one blocks the task until another `task.send`s.
struct Channel {
    queue: VecDeque<Value>,
    waiters: VecDeque<usize>,
}

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
/// `n` values with no value yet. When "no value" is all zero bytes in memory
/// (checked once), the memory comes zeroed from the system, which is much faster
/// for a large array than writing every element.
fn null_values(n: usize) -> Vec<Value> {
    static ZERO_IS_NULL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let zero = *ZERO_IS_NULL.get_or_init(|| {
        let null = Value::Null;
        // SAFETY: reading the bytes of a value; a zeroed buffer is checked to read as Null
        let first_zero = unsafe { *(&null as *const Value as *const u8) } == 0;
        let zeroed = std::mem::MaybeUninit::<Value>::zeroed();
        first_zero && matches!(unsafe { zeroed.assume_init_ref() }, Value::Null)
    });
    if !zero || n == 0 {
        return vec![Value::Null; n];
    }
    let Ok(layout) = std::alloc::Layout::array::<Value>(n) else { return vec![Value::Null; n] };
    // SAFETY: zeroed memory from the global allocator with Vec's own layout; every
    // element reads as Value::Null (checked above), which owns nothing
    unsafe {
        let p = std::alloc::alloc_zeroed(layout) as *mut Value;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Vec::from_raw_parts(p, n, n)
    }
}

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
pub(crate) fn repr(v: &Value) -> String {
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
        Value::Null => "none".to_string(),
        Value::Int(n) => n.to_string(),
        Value::Real(r) => format_real(*r),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Str(s) if s.contains('\'') && !s.contains('"') => format!("\"{}\"", s),
        Value::Str(s) => format!("'{}'", s.replace('\'', "\\'")),
        Value::Seq(q) => format!("[{}]", items(&mut q.borrow().iter())),
        Value::Packed(p) => format!("[{}]", items(&mut p.borrow().values().iter())),
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
        Value::Func(_) => "<function>".to_string(),
        Value::Set(t) => {
            let t = t.borrow();
            format!("{{{}}}", t.keys().map(|k| repr_at(&k.value(), depth + 1)).collect::<Vec<_>>().join(", "))
        }
        Value::Object(o) => {
            let o = o.borrow();
            let names = crate::tir::class_field_names(o.class);
            let pairs: Vec<String> =
                names.iter().zip(&o.fields).map(|(n, f)| format!("{}={}", n, repr_at(f, depth + 1))).collect();
            format!("{}({})", crate::tir::class_name(o.class), pairs.join(", "))
        }
        Value::Floats(g, shape) => {
            let g = g.borrow();
            let body = if g.len() <= 8 {
                format!("nungw[{}]", g.iter().map(|x| format_real(*x)).collect::<Vec<_>>().join(", "))
            } else {
                let head = g.iter().take(6).map(|x| format_real(*x)).collect::<Vec<_>>().join(", ");
                format!("nungw[{}, ... {} floats]", head, g.len())
            };
            // Append the shape only for genuinely multi-dimensional arrays, so
            // 1-D output stays exactly as before.
            if shape.len() > 1 {
                let dims = shape.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("x");
                format!("{body} ({dims})")
            } else {
                body
            }
        }
        #[cfg(feature = "gpu")]
        Value::GpuFloats(h, shape) => {
            let dims = if shape.len() > 1 {
                shape.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("x")
            } else {
                h.len.to_string()
            };
            format!("nungw[on GPU: {dims}]")
        }
    }
}

/// The compiler guarantees each instruction's operand types, so this never
/// runs; it is kept out of line so the hot paths that check stay small.
/// True for values that own no heap memory (dropping them does nothing).
#[inline(always)]
fn is_scalar(v: &Value) -> bool {
    matches!(v, Value::Null | Value::Int(_) | Value::Real(_) | Value::Bool(_) | Value::Func(_))
}

/// Removes the registers from `base` on (a returning frame), running a
/// destructor only for the values that own memory.
#[inline(always)]
fn release(regs: &mut Vec<Value>, base: usize) {
    if base >= regs.len() {
        return;
    }
    for v in &mut regs[base..] {
        if !is_scalar(v) {
            // SAFETY: each value is dropped once, then the length is cut below it
            unsafe { std::ptr::drop_in_place(v) }
        }
    }
    // SAFETY: the values past `base` were dropped (or own nothing)
    unsafe { regs.set_len(base) }
}

/// Adds a callee's registers at the end: its `argc` arguments, copied from
/// `start` (in the caller's registers), then "no value yet" up to `n_regs`.
#[inline(always)]
fn push_frame(regs: &mut Vec<Value>, start: usize, argc: usize, n_regs: usize) {
    let new_base = regs.len();
    assert!(start + argc <= new_base && argc <= n_regs, "internal error: bad call frame");
    regs.reserve(n_regs);
    // SAFETY: `reserve` made room for `n_regs` values past `new_base`; the
    // arguments are read from below `new_base`; every slot up to the new length
    // is written before the length is set
    unsafe {
        let p = regs.as_mut_ptr();
        for k in 0..argc {
            std::ptr::write(p.add(new_base + k), (*p.add(start + k)).clone());
        }
        for k in argc..n_regs {
            std::ptr::write(p.add(new_base + k), Value::Null);
        }
        regs.set_len(new_base + n_regs);
    }
}

#[cold]
#[inline(never)]
fn wrong_kind(expected: &str, found: &Value) -> ! {
    panic!("internal error: expected {}, found {:?}", expected, found)
}

#[inline(always)]
fn int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        other => wrong_kind("an integer", other),
    }
}

#[inline(always)]
fn real(v: &Value) -> f64 {
    match v {
        Value::Real(r) => *r,
        other => wrong_kind("a real", other),
    }
}

#[inline(always)]
fn boolean(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => wrong_kind("a boolean", other),
    }
}

#[inline(always)]
fn string(v: &Value) -> &str {
    match v {
        Value::Str(s) => s,
        other => wrong_kind("a string", other),
    }
}

#[inline(always)]
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

/// Would a run-time error end the program now (no try around, no other task)?
/// Then a parallel loop may write ahead of a turn that fails.
///
/// # Safety
/// `vm` is the running interpreter.
#[cfg(feature = "jit")]
pub(crate) unsafe fn jit_errors_end_program(vm: *mut u8) -> bool {
    let vm = &*(vm as *const Vm<'static>);
    vm.handlers.is_empty() && vm.tasks.len() <= 1
}

/// How many direct calls between compiled bodies may nest on the machine stack
/// before the interpreter takes the next one (the frames are the interpreter's).
#[cfg(feature = "jit")]
const NATIVE_CALLS: usize = 1000;

/// Compiled code calls function `callee` directly: the interpreter's frame is
/// built exactly as its own Call would (registers, return record), and the
/// callee's compiled body runs. Returns 0 when the callee returned (its value in
/// `out`, or in register `dst` when it is not a number), 1 when the interpreter
/// should make this call itself, and 2 when the callee stopped inside: its
/// registers are written back, its frame stays, and `jit_pending` says where the
/// interpreter goes on.
#[cfg(feature = "jit")]
unsafe extern "C" fn gw_call_body(
    ctx: *mut crate::jit::Ctx,
    callee: i64,
    first: i64,
    argc: i64,
    dst: i64,
    call_ip: i64,
    args: *const crate::jit::Slot,
    out: *mut crate::jit::Slot,
) -> i64 {
    use crate::jit::{Slot, TAG_NONE};
    let vm = &mut *((*ctx).vm as *mut Vm<'static>);
    let callee = callee as usize;
    if vm.frames.len() >= MAX_CALL_DEPTH || vm.jit_depth >= NATIVE_CALLS {
        return 1;
    }
    let jit: *mut crate::jit::Jit = match vm.jit.as_mut() {
        Some(j) => &mut **j,
        None => return 1,
    };
    let Some(body) = (*jit).entry(callee, 0) else { return 1 };
    let (entry, used, written) = (body.entry, &body.used as *const Vec<u16>, &body.written as *const Vec<u16>);
    let m = vm.module;
    let (caller, caller_base) = ((*ctx).func, (*ctx).base);
    let n_regs = m.functions[callee].n_regs as usize;
    // the callee's registers: the arguments, then "no value yet"
    let new_base = vm.regs.len();
    vm.regs.reserve(n_regs);
    for k in 0..n_regs {
        let v = if (k as i64) < argc {
            match (*args.add(k)).value() {
                Some(v) => v,
                None => vm.regs[caller_base + first as usize + k].clone(),
            }
        } else {
            Value::Null
        };
        vm.regs.push(v);
    }
    vm.frames.push(Frame { func: caller, ip: call_ip as usize + 1, base: caller_base, dst: dst as u16 });
    vm.jit_depth += 1;
    let d = vm.jit_depth;
    if vm.jit_pool.len() <= d {
        vm.jit_pool.resize_with(d + 1, Vec::new);
    }
    let mut slots = std::mem::take(&mut vm.jit_pool[d]);
    slots.clear();
    slots.resize(n_regs, Slot::default());
    for &r in &*used {
        slots[r as usize] = Slot::entry(&vm.regs[new_base + r as usize]);
    }
    let mut cx = crate::jit::Ctx {
        regs: vm.regs.as_mut_ptr().add(new_base),
        globals: (*ctx).globals,
        strings: (*ctx).strings,
        out: (*ctx).out,
        env: (*ctx).env,
        error: None,
        vm: (*ctx).vm,
        func: callee,
        base: new_base,
    };
    let budget = crate::jit::budget(vm.frames.len(), MAX_CALL_DEPTH);
    let resume = entry(slots.as_mut_ptr(), budget, &mut cx, 0);
    let vm = &mut *((*ctx).vm as *mut Vm<'static>);
    vm.jit_depth -= 1;
    let code = &m.functions[callee].code;
    let returned = resume != crate::jit::CALLEE_ACTIVE && cx.error.is_none() && matches!(code.get(resume as usize), Some(Op::Ret(_) | Op::RetNone));
    let status = if returned {
        // the callee returns: its value to the caller, its frame goes
        if let Some(Op::Ret(s)) = code.get(resume as usize) {
            let s = *s as usize;
            let v = match slots[s].value() {
                Some(v) => v,
                None => std::mem::replace(&mut vm.regs[new_base + s], Value::Null),
            };
            let sl = Slot::of(&v);
            if sl.tag == TAG_NONE {
                vm.regs[caller_base + dst as usize] = v;
            }
            *out = sl;
        }
        vm.frames.pop();
        release(&mut vm.regs, new_base);
        0
    } else {
        // the callee stopped: its registers back, its frame stays for the interpreter
        for &r in &*written {
            if let Some(v) = slots[r as usize].value() {
                vm.regs[new_base + r as usize] = v;
            }
        }
        if resume != crate::jit::CALLEE_ACTIVE {
            vm.jit_pending = Some((callee, new_base, resume as usize));
            if let Some(e) = cx.error.take() {
                vm.jit_pending_error = Some(e);
            }
        }
        2
    };
    vm.jit_pool[d] = slots;
    // the registers may have moved: the caller's view of its own
    (*ctx).regs = vm.regs.as_mut_ptr().add(caller_base);
    status
}

pub fn run(module: &Module, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
    let mut vm = Vm {
        module,
        #[cfg(feature = "jit")]
        jit: None,
        #[cfg(feature = "jit")]
        jit_slots: Vec::new(),
        #[cfg(feature = "jit")]
        jit_depth: 0,
        #[cfg(feature = "jit")]
        jit_pool: Vec::new(),
        #[cfg(feature = "jit")]
        jit_pending: None,
        #[cfg(feature = "jit")]
        jit_pending_error: None,
        regs: Vec::with_capacity(1 << 12),
        globals: vec![Value::Null; module.globals.len()],
        frames: Vec::new(),
        env: Env::default(),
        handlers: Vec::new(),
        tasks: Vec::new(),
        ready: VecDeque::new(),
        current: 0,
        channels: Vec::new(),
        prof: None,
        trace: None,
        debug: None,
    };
    // on with `import jit` (or GLOBW_JIT=1); GLOBW_NO_JIT=1 keeps it off
    #[cfg(feature = "jit")]
    if (module.jit || std::env::var_os("GLOBW_JIT").is_some()) && std::env::var_os("GLOBW_NO_JIT").is_none() {
        // the worker threads for parallel loops start now, while the program runs
        std::thread::spawn(|| rayon::current_num_threads());
        vm.jit = crate::jit::Jit::new(module, gw_call_body as *const u8).map(Box::new);
    }
    let result = vm.execute::<false>(input, output);
    let _ = output.flush();
    if result.is_ok() {
        vm.env.finish();
    }
    result
}

/// Runs the program, printing each source line as it executes (`--trace`): the
/// line number, the call-depth indentation, and the line's text, to stderr.
pub fn run_traced(
    module: &Module,
    files: &[crate::errors::SourceFile],
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<(), Error> {
    let lines: Vec<Vec<String>> =
        files.iter().map(|f| f.text.lines().map(|l| l.trim_end().to_string()).collect()).collect();
    let mut vm = Vm {
        module,
        #[cfg(feature = "jit")]
        jit: None,
        #[cfg(feature = "jit")]
        jit_slots: Vec::new(),
        #[cfg(feature = "jit")]
        jit_depth: 0,
        #[cfg(feature = "jit")]
        jit_pool: Vec::new(),
        #[cfg(feature = "jit")]
        jit_pending: None,
        #[cfg(feature = "jit")]
        jit_pending_error: None,
        regs: Vec::with_capacity(1 << 12),
        globals: vec![Value::Null; module.globals.len()],
        frames: Vec::new(),
        env: Env::default(),
        handlers: Vec::new(),
        tasks: Vec::new(),
        ready: VecDeque::new(),
        current: 0,
        channels: Vec::new(),
        prof: None,
        trace: Some(lines),
        debug: None,
    };
    let result = vm.execute::<true>(input, output);
    let _ = output.flush();
    if result.is_ok() {
        vm.env.finish();
    }
    result
}

/// Runs the program under a debugger: `hook` is called at every new source line
/// and may stop there to inspect the state (see `crate::debug`).
pub fn run_debug<'m>(
    module: &'m Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    hook: Box<dyn crate::debug::DebugHook + 'm>,
) -> Result<(), Error> {
    let mut vm = Vm {
        module,
        #[cfg(feature = "jit")]
        jit: None,
        #[cfg(feature = "jit")]
        jit_slots: Vec::new(),
        #[cfg(feature = "jit")]
        jit_depth: 0,
        #[cfg(feature = "jit")]
        jit_pool: Vec::new(),
        #[cfg(feature = "jit")]
        jit_pending: None,
        #[cfg(feature = "jit")]
        jit_pending_error: None,
        regs: Vec::with_capacity(1 << 12),
        globals: vec![Value::Null; module.globals.len()],
        frames: Vec::new(),
        env: Env::default(),
        handlers: Vec::new(),
        tasks: Vec::new(),
        ready: VecDeque::new(),
        current: 0,
        channels: Vec::new(),
        prof: None,
        trace: None,
        debug: Some(hook),
    };
    let result = vm.execute::<true>(input, output);
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
        #[cfg(feature = "jit")]
        jit: None,
        #[cfg(feature = "jit")]
        jit_slots: Vec::new(),
        #[cfg(feature = "jit")]
        jit_depth: 0,
        #[cfg(feature = "jit")]
        jit_pool: Vec::new(),
        #[cfg(feature = "jit")]
        jit_pending: None,
        #[cfg(feature = "jit")]
        jit_pending_error: None,
        regs: Vec::with_capacity(1 << 12),
        globals: vec![Value::Null; module.globals.len()],
        frames: Vec::new(),
        env: Env::default(),
        handlers: Vec::new(),
        tasks: Vec::new(),
        ready: VecDeque::new(),
        current: 0,
        channels: Vec::new(),
        prof: Some(HashMap::new()),
        trace: None,
        debug: None,
    };
    let result = vm.execute::<true>(input, output);
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
    /// machine code for hot numeric functions and loops (normal runs only)
    #[cfg(feature = "jit")]
    jit: Option<Box<crate::jit::Jit>>,
    #[cfg(feature = "jit")]
    jit_slots: Vec<crate::jit::Slot>,
    /// direct calls between compiled bodies: how deep they nest on the machine
    /// stack, a register buffer per level, and where to go on when one stops
    #[cfg(feature = "jit")]
    jit_depth: usize,
    #[cfg(feature = "jit")]
    jit_pool: Vec<Vec<crate::jit::Slot>>,
    #[cfg(feature = "jit")]
    jit_pending: Option<(usize, usize, usize)>,
    #[cfg(feature = "jit")]
    jit_pending_error: Option<String>,
    regs: Vec<Value>,
    globals: Vec<Value>,
    frames: Vec<Frame>,
    env: Env,
    /// active error handlers (try blocks), innermost last
    handlers: Vec<Handler>,
    /// cooperative tasks (green threads); the running one's live state is in the
    /// fields above, its slot here is empty until it is suspended
    tasks: Vec<TaskSlot>,
    /// handles of tasks ready to run, in round-robin order
    ready: VecDeque<usize>,
    /// the handle of the task running right now
    current: usize,
    /// inter-task channels
    channels: Vec<Channel>,
    /// when profiling: how many times each opcode ran
    prof: Option<HashMap<&'static str, u64>>,
    /// when tracing (`--trace`): the source lines of each file, for printing the
    /// line being executed
    trace: Option<Vec<Vec<String>>>,
    /// when debugging: the debugger, called at every new source line
    debug: Option<Box<dyn crate::debug::DebugHook + 'm>>,
}

/// An installed `try` handler: where to resume and the VM depth to restore.
struct Handler {
    catch_ip: usize,
    err: u16,
    frames: usize,
    regs: usize,
    func: usize,
    base: usize,
}

impl<'m> Vm<'m> {
    fn error_at(&self, func: usize, ip: usize, base: usize, message: impl Into<String>) -> Error {
        let e = Error::new(self.module.functions[func].positions[ip - 1], message);
        match self.backtrace(func, ip, base) {
            Some(trace) => e.with_trace(trace),
            None => e,
        }
    }

    /// The state a debugger can read, at the instruction just before `ip`.
    fn debug_state(&self, func: usize, ip: usize, base: usize, returned: bool) -> crate::debug::DebugState<'_> {
        let mut frames = vec![(func, ip, base)];
        frames.extend(self.frames.iter().rev().map(|fr| (fr.func, fr.ip, fr.base)));
        crate::debug::DebugState {
            module: self.module,
            regs: &self.regs,
            globals: &self.globals,
            frames,
            depth: self.frames.len(),
            returned,
        }
    }

    fn line_at(&self, func: usize, ip: usize) -> u32 {
        self.module.functions[func].positions[ip.saturating_sub(1)].line
    }

    /// A frame's label: the function name with its parameters and their current
    /// values, like `inner(x=0)`, or "the main program".
    fn frame_label(&self, func: usize, base: usize) -> String {
        let f = &self.module.functions[func];
        if f.name.is_empty() {
            return "the main program".to_string();
        }
        let args: Vec<String> = (0..f.n_params as usize)
            .map(|k| {
                let name = f.locals.get(k).map_or("?", String::as_str);
                let mut v = self.regs.get(base + k).map(display).unwrap_or_default();
                if v.chars().count() > 40 {
                    v = format!("{}…", v.chars().take(39).collect::<String>());
                }
                format!("{}={}", name, v)
            })
            .collect();
        format!("{}({})", f.name, args.join(", "))
    }

    /// The call stack at the point of an uncaught run-time error, as text. None
    /// when the error is in the main program (the single error line is enough).
    fn backtrace(&self, func: usize, ip: usize, base: usize) -> Option<String> {
        if self.frames.is_empty() {
            return None;
        }
        let mut s = String::from("Call stack (most recent first):\n");
        s.push_str(&format!("  in {} at line {}\n", self.frame_label(func, base), self.line_at(func, ip)));
        for fr in self.frames.iter().rev() {
            s.push_str(&format!("  called from {} at line {}\n", self.frame_label(fr.func, fr.base), self.line_at(fr.func, fr.ip)));
        }
        Some(s)
    }

    /// Removes handlers left by a function that has just returned.
    fn drop_handlers_above(&mut self) {
        while self.handlers.last().is_some_and(|h| h.frames > self.frames.len()) {
            self.handlers.pop();
        }
    }

    /// Runs the program. `WATCH` builds the copy of the loop that profiles,
    /// traces or debugs; the plain copy (`false`) has none of those checks.
    fn execute<const WATCH: bool>(&mut self, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), Error> {
        let m = self.module;
        let mut func = 0usize;
        let mut code: &[Op] = &m.functions[0].code;
        let mut ip = 0usize;
        let mut base = 0usize;
        let mut trace_last: (u16, u32) = (u16::MAX, 0); // last (file, line) traced
        // try to go on in compiled code after this instruction
        #[allow(unused_mut, unused_variables)]
        let mut jit_try = false;
        // an error a built-in raised inside compiled code, reported at the next turn
        #[allow(unused_mut)]
        let mut jit_error: Option<String> = None;
        let mut debug_last: (u16, u32, usize) = (u16::MAX, 0, 0); // last (file, line, depth) debugged
        self.regs.resize(m.functions[0].n_regs as usize, Value::Null);
        // task 0 is the main program; its live state is in the locals above and in
        // self, so its slot stays empty until it is suspended
        self.tasks.push(TaskSlot::empty());
        self.current = 0;

        macro_rules! reg {
            ($r:expr) => {
                self.regs[base + $r as usize]
            };
        }
        // Writes a number or bool into a register. When the old value is a
        // scalar too there is nothing to free, so its destructor is skipped.
        macro_rules! put {
            ($r:expr, $v:expr) => {{
                let v: Value = $v;
                let slot = &mut self.regs[base + $r as usize];
                if matches!(slot, Value::Null | Value::Int(_) | Value::Real(_) | Value::Bool(_)) {
                    // SAFETY: the old value owns no heap memory, so overwriting it
                    // without running its destructor leaks nothing
                    unsafe { std::ptr::write(slot, v) }
                } else {
                    *slot = v;
                }
            }};
        }
        // Stores any value into a register, skipping the old value's destructor
        // when it is a scalar (which owns nothing).
        macro_rules! set {
            ($r:expr, $v:expr) => {{
                let v: Value = $v;
                let slot = &mut self.regs[base + $r as usize];
                if is_scalar(slot) {
                    // SAFETY: a scalar owns no heap memory: nothing leaks
                    unsafe { std::ptr::write(slot, v) }
                } else {
                    *slot = v;
                }
            }};
        }
        // A loop jumped back from instruction `$at` to `$t`: one more turn makes its
        // function hotter; once compiled, the body runs from here.
        macro_rules! back_edge {
            ($t:expr, $at:expr) => {
                #[cfg(feature = "jit")]
                if !WATCH && ($t) <= ($at) {
                    if let Some(jit) = self.jit.as_mut() {
                        jit.heat(m, func, 1, &self.regs[base..]);
                        jit_try = true;
                    }
                }
            };
        }
        macro_rules! fail {
            ($msg:expr) => {
                break Some(($msg).to_string())
            };
        }
        // Saves the running task's live state into its slot (keeping `done`), so
        // another task can run; then its slot holds everything to resume it.
        macro_rules! save_current {
            () => {{
                let cur = self.current;
                self.tasks[cur].regs = std::mem::take(&mut self.regs);
                self.tasks[cur].frames = std::mem::take(&mut self.frames);
                self.tasks[cur].handlers = std::mem::take(&mut self.handlers);
                self.tasks[cur].func = func;
                self.tasks[cur].ip = ip;
                self.tasks[cur].base = base;
            }};
        }
        // Makes `$idx` the running task: loads its state into the locals and self.
        macro_rules! load_task {
            ($idx:expr) => {{
                // move the execution state into self, but leave the task's metadata
                // (waiters, done) in the slot so it survives while the task runs
                self.regs = std::mem::take(&mut self.tasks[$idx].regs);
                self.frames = std::mem::take(&mut self.tasks[$idx].frames);
                self.handlers = std::mem::take(&mut self.tasks[$idx].handlers);
                func = self.tasks[$idx].func;
                ip = self.tasks[$idx].ip;
                base = self.tasks[$idx].base;
                code = &m.functions[func].code;
                self.current = $idx;
            }};
        }
        // The running task's top function has returned: mark it done and run the
        // next ready task, or finish the program if none are left.
        macro_rules! finish_task {
            () => {{
                self.tasks[self.current].done = true;
                // wake everyone that was blocked in task.await on this task
                let waiters = std::mem::take(&mut self.tasks[self.current].waiters);
                self.ready.extend(waiters);
                match self.ready.pop_front() {
                    Some(next) => load_task!(next),
                    None => return Ok(()),
                }
            }};
        }

        loop {
            let op = code[ip];
            ip += 1;
            // profiling, tracing and debugging: compiled only into the watched
            // copy of this loop, so a normal run pays nothing for them
            if WATCH {
                if let Some(p) = self.prof.as_mut() {
                    *p.entry(op.name()).or_insert(0) += 1;
                }
                if let Some(files) = &self.trace {
                    let pos = m.functions[func].positions[ip - 1];
                    if pos.line != 0 && (pos.file, pos.line) != trace_last {
                        trace_last = (pos.file, pos.line);
                        let src = files
                            .get(pos.file as usize)
                            .and_then(|ls| ls.get(pos.line.saturating_sub(1) as usize))
                            .map_or("", |s| s.trim());
                        let indent = "  ".repeat(self.frames.len().min(24));
                        eprintln!("{:>4} | {}{}", pos.line, indent, src);
                    }
                }
                if self.debug.is_some() {
                    let pos = m.functions[func].positions[ip - 1];
                    let depth = self.frames.len();
                    if pos.line != 0 && (pos.file, pos.line, depth) != debug_last {
                        let returned = debug_last.0 != u16::MAX && depth < debug_last.2;
                        debug_last = (pos.file, pos.line, depth);
                        let mut hook = self.debug.take().expect("a debugger");
                        let state = self.debug_state(func, ip, base, returned);
                        let action = hook.on_line(&state);
                        drop(state);
                        self.debug = Some(hook);
                        if action == crate::debug::DebugAction::Terminate {
                            return Ok(());
                        }
                    }
                }
            }
            let raised: Option<String> = if let Some(msg) = jit_error.take() { Some(msg) } else { loop {
                match op {
                Op::PushHandler(catch_ip, err) => self.handlers.push(Handler {
                    catch_ip: catch_ip as usize,
                    err,
                    frames: self.frames.len(),
                    regs: self.regs.len(),
                    func,
                    base,
                }),
                Op::PopHandler => {
                    self.handlers.pop();
                }
                Op::Fail(r) => fail!(string(&reg!(r))),
                Op::LoadInt(d, n) => put!(d, Value::Int(n)),
                Op::LoadReal(d, r) => put!(d, Value::Real(r)),
                Op::LoadBool(d, b) => put!(d, Value::Bool(b)),
                Op::LoadStr(d, i) => reg!(d) = Value::Str(m.strings[i as usize].clone()),
                Op::Move(d, s) => {
                    let v = reg!(s).clone();
                    set!(d, v);
                }
                Op::AddI(d, a, b) => match int(&reg!(a)).checked_add(int(&reg!(b))) {
                    Some(n) => put!(d, Value::Int(n)),
                    None => fail!(OVERFLOW),
                },
                Op::SubI(d, a, b) => match int(&reg!(a)).checked_sub(int(&reg!(b))) {
                    Some(n) => put!(d, Value::Int(n)),
                    None => fail!(OVERFLOW),
                },
                Op::MulI(d, a, b) => match int(&reg!(a)).checked_mul(int(&reg!(b))) {
                    Some(n) => put!(d, Value::Int(n)),
                    None => fail!(OVERFLOW),
                },
                Op::DivI(d, a, b) => {
                    let (x, y) = (int(&reg!(a)), int(&reg!(b)));
                    if y == 0 {
                        fail!("division by zero");
                    }
                    match x.checked_div(y) {
                        Some(n) => put!(d, Value::Int(n)),
                        None => fail!(OVERFLOW),
                    }
                }
                Op::ModI(d, a, b) => {
                    let (x, y) = (int(&reg!(a)), int(&reg!(b)));
                    if y == 0 {
                        fail!("division by zero");
                    }
                    match x.checked_rem(y) {
                        Some(n) => put!(d, Value::Int(n)),
                        None => fail!(OVERFLOW),
                    }
                }
                Op::AddR(d, a, b) => put!(d, Value::Real(real(&reg!(a)) + real(&reg!(b)))),
                Op::SubR(d, a, b) => put!(d, Value::Real(real(&reg!(a)) - real(&reg!(b)))),
                Op::MulR(d, a, b) => put!(d, Value::Real(real(&reg!(a)) * real(&reg!(b)))),
                Op::DivR(d, a, b) => {
                    let y = real(&reg!(b));
                    if y == 0.0 {
                        fail!("division by zero");
                    }
                    put!(d, Value::Real(real(&reg!(a)) / y));
                }
                Op::Concat(d, a, b) => {
                    let mut t = String::with_capacity(string(&reg!(a)).len() + string(&reg!(b)).len());
                    t.push_str(string(&reg!(a)));
                    t.push_str(string(&reg!(b)));
                    reg!(d) = Value::Str(t.into());
                }
                Op::NegI(d, s) => match int(&reg!(s)).checked_neg() {
                    Some(n) => put!(d, Value::Int(n)),
                    None => fail!(OVERFLOW),
                },
                Op::NegR(d, s) => put!(d, Value::Real(-real(&reg!(s)))),
                Op::Not(d, s) => put!(d, Value::Bool(!boolean(&reg!(s)))),
                Op::ToReal(d, s) => put!(d, Value::Real(int(&reg!(s)) as f64)),
                Op::ToStr(d, s) => {
                    let t = display(&reg!(s));
                    reg!(d) = Value::Str(t.into());
                }
                Op::CmpI(c, d, a, b) => {
                    let r = compare(c, &int(&reg!(a)), &int(&reg!(b)));
                    put!(d, Value::Bool(r));
                }
                Op::CmpR(c, d, a, b) => {
                    let r = compare(c, &real(&reg!(a)), &real(&reg!(b)));
                    put!(d, Value::Bool(r));
                }
                Op::CmpS(c, d, a, b) => {
                    let r = compare::<str>(c, string(&reg!(a)), string(&reg!(b)));
                    put!(d, Value::Bool(r));
                }
                Op::CmpB(c, d, a, b) => {
                    let r = compare(c, &boolean(&reg!(a)), &boolean(&reg!(b)));
                    put!(d, Value::Bool(r));
                }
                Op::Jump(t) => {
                    let at = ip - 1;
                    ip = t as usize;
                    back_edge!(t as usize, at);
                }
                Op::JumpIfFalse(c, t) => {
                    if !boolean(&reg!(c)) {
                        let at = ip - 1;
                        ip = t as usize;
                        back_edge!(t as usize, at);
                    }
                }
                Op::JumpIfTrue(c, t) => {
                    if boolean(&reg!(c)) {
                        let at = ip - 1;
                        ip = t as usize;
                        back_edge!(t as usize, at);
                    }
                }
                Op::JumpUnlessI(c, a, b, t) => {
                    if !compare(c, &int(&reg!(a)), &int(&reg!(b))) {
                        let at = ip - 1;
                        ip = t as usize;
                        back_edge!(t as usize, at);
                    }
                }
                Op::JumpUnlessIImm(c, a, n, t) => {
                    if !compare(c, &int(&reg!(a)), &n) {
                        let at = ip - 1;
                        ip = t as usize;
                        back_edge!(t as usize, at);
                    }
                }
                Op::CmpIImm(c, d, a, n) => put!(d, Value::Bool(compare(c, &int(&reg!(a)), &n))),
                Op::AddIImm(d, a, n) => match int(&reg!(a)).checked_add(n) {
                    Some(v) => put!(d, Value::Int(v)),
                    None => fail!(OVERFLOW),
                },
                Op::SubIImm(d, a, n) => match int(&reg!(a)).checked_sub(n) {
                    Some(v) => put!(d, Value::Int(v)),
                    None => fail!(OVERFLOW),
                },
                Op::MulIImm(d, a, n) => match int(&reg!(a)).checked_mul(n) {
                    Some(v) => put!(d, Value::Int(v)),
                    None => fail!(OVERFLOW),
                },
                Op::DivIImm(d, a, n) => match int(&reg!(a)).checked_div(n) {
                    Some(v) => put!(d, Value::Int(v)),
                    None => fail!(OVERFLOW),
                },
                Op::ModIImm(d, a, n) => match int(&reg!(a)).checked_rem(n) {
                    Some(v) => put!(d, Value::Int(v)),
                    None => fail!(OVERFLOW),
                },
                Op::ForInit { var, end, step, exit } => {
                    {
                        let s = int(&reg!(step));
                        if s == 0 {
                            fail!("step cannot be 0");
                        }
                        let (v, e) = (int(&reg!(var)), int(&reg!(end)));
                        if (s > 0 && v > e) || (s < 0 && v < e) {
                            ip = exit as usize;
                        }
                    }
                }
                Op::ForStep { var, end, step, body } => {
                    let s = int(&reg!(step));
                    // on overflow the counter is past any end value: the loop is over
                    if let Some(v) = int(&reg!(var)).checked_add(s) {
                        let e = int(&reg!(end));
                        put!(var, Value::Int(v));
                        if (s > 0 && v <= e) || (s < 0 && v >= e) {
                            let at = ip - 1;
                            ip = body as usize;
                            back_edge!(body as usize, at);
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
                Op::NewArray(d, n) => reg!(d) = Value::Seq(Rc::new(RefCell::new(null_values(n as usize)))),
                Op::NewPacked(d, n, kind) => reg!(d) = Value::Packed(Rc::new(RefCell::new(Packed::new(n as usize, kind)))),
                Op::NewMatrix(d, r, c) => {
                    let (rows, cols) = (r as usize, c as usize);
                    let data = null_values(rows * cols);
                    reg!(d) = Value::Matrix(Rc::new(RefCell::new(Matrix { rows, cols, data })));
                }
                Op::NewList(d) => reg!(d) = Value::Seq(Rc::new(RefCell::new(Vec::new()))),
                Op::NewDict(d) => reg!(d) = Value::Dict(Rc::new(RefCell::new(Dict::default()))),
                Op::NewSet(d) => reg!(d) = Value::Set(Rc::new(RefCell::new(Dict::default()))),
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
                    if matches!(reg!(o), Value::Null) {
                        fail!("this object has no value (it is none)");
                    }
                    // an unset object or function field reads as `none`
                    let v = object(&reg!(o)).borrow().fields[slot as usize].clone();
                    reg!(d) = v;
                }
                Op::SetField(o, slot, s) => {
                    if matches!(reg!(o), Value::Null) {
                        fail!("this object has no value (it is none)");
                    }
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
                    let v = if let Value::Packed(p) = &reg!(c) {
                        let p = p.borrow();
                        match position(i, p.len()) {
                            Ok(k) => p.get(k),
                            Err(msg) => fail!(msg),
                        }
                    } else {
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
                    if let Value::Packed(p) = &reg!(c) {
                        let mut p = p.borrow_mut();
                        match position(i, p.len()) {
                            Ok(k) => p.set(k, &reg!(s)),
                            Err(msg) => fail!(msg),
                        }
                    } else {
                    let v = reg!(s).clone();
                    let mut q = seq(&reg!(c)).borrow_mut();
                    match position(i, q.len()) {
                        Ok(k) => q[k] = v,
                        Err(msg) => fail!(msg),
                    }
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
                        Value::Packed(p) => p.borrow().len(),
                        Value::Dict(t) => t.borrow().len(),
                        Value::Set(t) => t.borrow().len(),
                        Value::Str(t) => t.chars().count(),
                        other => panic!("internal error: length of {:?}", other),
                    };
                    put!(d, Value::Int(n as i64));
                }
                Op::Append(l, s) => {
                    let v = reg!(s).clone();
                    if let Value::Packed(p) = &reg!(l) {
                        p.borrow_mut().push(&v);
                    } else {
                    seq(&reg!(l)).borrow_mut().push(v);
                    }
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
                Op::CallMethod { slot, first, argc, dst } => {
                    if self.frames.len() >= MAX_CALL_DEPTH {
                        fail!("too many nested calls (maybe a recursion that never ends)");
                    }
                    if matches!(self.regs[base + first as usize], Value::Null) {
                        fail!("this object has no value (it is none)");
                    }
                    let class = object(&self.regs[base + first as usize]).borrow().class as usize;
                    let f = m.vtable[class][slot as usize] as usize;
                    let callee = &m.functions[f];
                    let new_base = self.regs.len();
                    let n_regs = callee.n_regs as usize;
                    let start = base + first as usize;
                    push_frame(&mut self.regs, start, argc as usize, n_regs);
                    self.frames.push(Frame { func, ip, base, dst });
                    func = f;
                    code = &callee.code;
                    ip = 0;
                    base = new_base;
                }
                Op::IsClass(d, o, cls) => {
                    let mut c = object(&reg!(o)).borrow().class as i32;
                    let target = cls as i32;
                    let mut found = false;
                    while c >= 0 {
                        if c == target {
                            found = true;
                            break;
                        }
                        c = m.class_parents[c as usize];
                    }
                    put!(d, Value::Bool(found));
                }
                Op::LoadNull(d) => reg!(d) = Value::Null,
                Op::IsNull(d, r) => put!(d, Value::Bool(matches!(reg!(r), Value::Null))),
                Op::LoadFunc(d, index) => reg!(d) = Value::Func(index),
                Op::CallValue { func: fr, first, argc, dst } => {
                    if self.frames.len() >= MAX_CALL_DEPTH {
                        fail!("too many nested calls (maybe a recursion that never ends)");
                    }
                    let f = match reg!(fr) {
                        Value::Func(i) => i as usize,
                        ref other => panic!("internal error: called a non-function {:?}", other),
                    };
                    let callee = &m.functions[f];
                    let new_base = self.regs.len();
                    let n_regs = callee.n_regs as usize;
                    let start = base + first as usize;
                    push_frame(&mut self.regs, start, argc as usize, n_regs);
                    self.frames.push(Frame { func, ip, base, dst });
                    func = f;
                    code = &callee.code;
                    ip = 0;
                    base = new_base;
                }
                Op::Call { func: f, first, argc, dst } => {
                    let mut done = false;
                    // a compiled function: run its machine code; if it gives up, the
                    // call goes on in the interpreter below
                    #[cfg(feature = "jit")]
                    if !WATCH {
                        if let Some(jit) = self.jit.as_mut() {
                            if let Some(entry) = jit.function(m, f as usize) {
                                let start = base + first as usize;
                                let args: Vec<crate::jit::Slot> = (0..argc as usize).map(|k| crate::jit::Slot::of(&self.regs[start + k])).collect();
                                if args.iter().all(|a| a.tag != crate::jit::TAG_NONE) {
                                    let mut out = crate::jit::Slot::default();
                                    let budget = crate::jit::budget(self.frames.len(), MAX_CALL_DEPTH);
                                    // SAFETY: the trampoline reads `argc` slots and writes one
                                    unsafe { entry(args.as_ptr(), budget, &mut out) };
                                    if let Some(v) = out.value() {
                                        set!(dst, v);
                                        done = true;
                                    } else {
                                        jit.function_bailed(f as usize);
                                    }
                                }
                            }
                        }
                    }
                    if !done {
                    if self.frames.len() >= MAX_CALL_DEPTH {
                        fail!("too many nested calls (maybe a recursion that never ends)");
                    }
                    let callee = &m.functions[f as usize];
                    let new_base = self.regs.len();
                    let n_regs = callee.n_regs as usize;
                    // push the arguments straight into the callee's registers, then
                    // fill the remaining locals with "no value yet" in one pass
                    let start = base + first as usize;
                    push_frame(&mut self.regs, start, argc as usize, n_regs);
                    self.frames.push(Frame { func, ip, base, dst });
                    func = f as usize;
                    code = &callee.code;
                    ip = 0;
                    base = new_base;
                    #[cfg(feature = "jit")]
                    if !WATCH {
                        if let Some(jit) = self.jit.as_mut() {
                            jit.heat(m, func, 25, &self.regs[base..]);
                            jit_try = true;
                        }
                    }
                    }
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
                    match self.frames.pop() {
                        Some(fr) => {
                            release(&mut self.regs, base);
                            func = fr.func;
                            code = &m.functions[func].code;
                            ip = fr.ip;
                            base = fr.base;
                            set!(fr.dst, v);
                            self.drop_handlers_above();
                            jit_try = true;
                        }
                        None => finish_task!(),
                    }
                }
                Op::RetNone => match self.frames.pop() {
                    Some(fr) => {
                        release(&mut self.regs, base);
                        func = fr.func;
                        code = &m.functions[func].code;
                        ip = fr.ip;
                        base = fr.base;
                        self.drop_handlers_above();
                        jit_try = true;
                    }
                    None => finish_task!(),
                },
                Op::Builtin { id, first, argc, dst } => {
                    let b = Builtin::from_id(id);
                    match b {
                        Builtin::TaskSpawn => {
                            let start = base + first as usize;
                            let fidx = match &self.regs[start] {
                                Value::Func(i) => *i as usize,
                                _ => fail!("task.spawn needs a function to run"),
                            };
                            if self.tasks.len() >= MAX_TASKS {
                                fail!("too many tasks (a runaway task.spawn?)");
                            }
                            let n = m.functions[fidx].n_regs as usize;
                            let handle = self.tasks.len();
                            self.tasks.push(TaskSlot {
                                regs: vec![Value::Null; n],
                                frames: Vec::new(),
                                handlers: Vec::new(),
                                func: fidx,
                                ip: 0,
                                base: 0,
                                done: false,
                                waiters: Vec::new(),
                            });
                            self.ready.push_back(handle);
                            put!(dst, Value::Int(handle as i64));
                        }
                        Builtin::TaskYield => {
                            if let Some(next) = self.ready.pop_front() {
                                save_current!();
                                self.ready.push_back(self.current);
                                load_task!(next);
                            }
                        }
                        Builtin::TaskAwait => {
                            let start = base + first as usize;
                            let h = match self.regs[start] {
                                Value::Int(i) if i >= 0 && (i as usize) < self.tasks.len() => i as usize,
                                _ => fail!("task.await: that is not a task handle"),
                            };
                            // block until task h finishes, unless it is already done or is us
                            if !self.tasks[h].done && h != self.current {
                                save_current!();
                                self.tasks[h].waiters.push(self.current);
                                match self.ready.pop_front() {
                                    Some(next) => load_task!(next),
                                    None => fail!("deadlock: a task is waiting for one that can never run"),
                                }
                            }
                        }
                        Builtin::TaskDone => {
                            let start = base + first as usize;
                            let done = match self.regs[start] {
                                Value::Int(i) if i >= 0 && (i as usize) < self.tasks.len() => self.tasks[i as usize].done,
                                _ => fail!("task.done: that is not a task handle"),
                            };
                            put!(dst, Value::Bool(done));
                        }
                        Builtin::TaskId => put!(dst, Value::Int(self.current as i64)),
                        Builtin::TaskCount => put!(dst, Value::Int(self.tasks.len() as i64)),
                        Builtin::TaskChannel => {
                            let h = self.channels.len();
                            self.channels.push(Channel { queue: VecDeque::new(), waiters: VecDeque::new() });
                            put!(dst, Value::Int(h as i64));
                        }
                        Builtin::TaskSend => {
                            let start = base + first as usize;
                            let ch = match self.regs[start] {
                                Value::Int(i) if i >= 0 && (i as usize) < self.channels.len() => i as usize,
                                _ => fail!("task.send: that is not a channel"),
                            };
                            let v = self.regs[start + 1].clone();
                            self.channels[ch].queue.push_back(v);
                            if let Some(w) = self.channels[ch].waiters.pop_front() {
                                self.ready.push_back(w);
                            }
                        }
                        Builtin::TaskRecv => {
                            let start = base + first as usize;
                            let ch = match self.regs[start] {
                                Value::Int(i) if i >= 0 && (i as usize) < self.channels.len() => i as usize,
                                _ => fail!("task.recv: that is not a channel"),
                            };
                            match self.channels[ch].queue.pop_front() {
                                Some(v) => reg!(dst) = v,
                                None => {
                                    // block until a value is sent, then re-run this op
                                    ip -= 1;
                                    save_current!();
                                    self.channels[ch].waiters.push_back(self.current);
                                    match self.ready.pop_front() {
                                        Some(next) => load_task!(next),
                                        None => fail!("deadlock: task.recv on an empty channel and no task can send"),
                                    }
                                }
                            }
                        }
                        Builtin::TaskHas => {
                            let start = base + first as usize;
                            let has = match self.regs[start] {
                                Value::Int(i) if i >= 0 && (i as usize) < self.channels.len() => {
                                    !self.channels[i as usize].queue.is_empty()
                                }
                                _ => fail!("task.has: that is not a channel"),
                            };
                            put!(dst, Value::Bool(has));
                        }
                        _ => {
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
                break None;
            } };
            if let Some(msg) = raised {
                // a run-time error: jump to the nearest try's catch, or stop
                match self.handlers.pop() {
                    Some(h) => {
                        self.frames.truncate(h.frames);
                        self.regs.truncate(h.regs);
                        func = h.func;
                        code = &m.functions[func].code;
                        base = h.base;
                        ip = h.catch_ip;
                        self.regs[base + h.err as usize] = Value::Str(msg.as_str().into());
                        jit_try = true;
                    }
                    None => {
                        if let Some(mut hook) = self.debug.take() {
                            // let the debugger show the failing line and its variables
                            let state = self.debug_state(func, ip, base, false);
                            hook.on_error(&state, &msg);
                        }
                        return Err(self.error_at(func, ip, base, msg));
                    }
                }
            }
            // go on in the function's compiled body, from this instruction
            #[cfg(feature = "jit")]
            if jit_try && !WATCH {
                jit_try = false;
                let jit: *mut crate::jit::Jit = match self.jit.as_mut() {
                    Some(j) => &mut **j,
                    None => std::ptr::null_mut(),
                };
                if !jit.is_null() {
                    // SAFETY: no Rust reference into the JIT lives across the call;
                    // compiled code reaches the VM only through `vm`
                    let body = unsafe { (*jit).entry(func, ip) }.map(|b| (b.entry, &b.used as *const Vec<u16>, &b.written as *const Vec<u16>));
                    if let Some((entry, used, written)) = body {
                        let n = m.functions[func].n_regs as usize;
                        self.jit_slots.clear();
                        self.jit_slots.resize(n, crate::jit::Slot::default());
                        for &r in unsafe { &*used } {
                            self.jit_slots[r as usize] = crate::jit::Slot::entry(&self.regs[base + r as usize]);
                        }
                        let vm_ptr = self as *mut Vm as *mut u8;
                        let mut cx = crate::jit::Ctx {
                            regs: self.regs.as_mut_ptr().wrapping_add(base),
                            globals: self.globals.as_mut_ptr(),
                            strings: m.strings.as_ptr(),
                            // SAFETY: used only while the compiled body runs, inside this call
                            out: unsafe { std::mem::transmute::<*mut (dyn Write + '_), *mut (dyn Write + 'static)>(output as *mut dyn Write) },
                            env: &mut self.env,
                            error: None,
                            vm: vm_ptr,
                            func,
                            base,
                        };
                        let budget = crate::jit::budget(self.frames.len(), MAX_CALL_DEPTH);
                        let slots = self.jit_slots.as_mut_ptr();
                        // SAFETY: the body reads and writes its slots, and through the
                        // helpers this frame's registers, the globals and (direct calls) the VM
                        let resume = unsafe { entry(slots, budget, &mut cx, ip as i64) };
                        for &r in unsafe { &*written } {
                            if let Some(v) = self.jit_slots[r as usize].value() {
                                self.regs[base + r as usize] = v;
                            }
                        }
                        jit_try = true;
                        if resume == crate::jit::CALLEE_ACTIVE {
                            // a callee stopped: its frame is on top, go on there
                            if let Some((f2, b2, ip2)) = self.jit_pending.take() {
                                func = f2;
                                base = b2;
                                ip = ip2;
                                code = &m.functions[func].code;
                            }
                            if let Some(msg) = self.jit_pending_error.take() {
                                jit_error = Some(msg);
                            }
                        } else {
                            let resume = resume as usize;
                            // stopped at an instruction it can run: an error to report,
                            // or a call that gave up
                            if unsafe { (*jit).runs_at(func, resume) } {
                                unsafe { (*jit).stopped(func, resume) };
                            }
                            ip = resume;
                            if let Some(msg) = cx.error.take() {
                                // the next turn fetches that instruction and reports this
                                jit_error = Some(msg);
                            }
                        }
                    } else if unsafe { (*jit).compiled(func) } {
                        jit_try = true;
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
            functions: vec![Function { name: "main".into(), n_params: 0, n_regs, code, positions, locals: vec![] }],
            strings: strings.iter().map(|s| Rc::from(*s)).collect(),
            globals: vec![],
            class_fields: vec![],
            vtable: vec![],
            class_parents: vec![],
            jit: false,
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
                Function { name: "main".into(), n_params: 0, n_regs: main_regs, positions: pos(main.len()), code: main, locals: vec![] },
                Function { name: "f".into(), n_params: 1, n_regs: f_regs, positions: pos(f.len()), code: f, locals: vec![] },
            ],
            strings: vec![],
            globals: vec!["g".into()],
            class_fields: vec![],
            vtable: vec![],
            class_parents: vec![],
            jit: false,
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
