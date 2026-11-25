//! The JIT: hot code becomes machine code, through Cranelift.
//!
//! Two kinds of unit are compiled:
//! - **functions** made only of numeric instructions (ints, reals, bools, jumps,
//!   calls to other such functions): they become native functions that call each
//!   other directly, every value in a machine register. They change nothing but
//!   their own registers, so when one meets something it does not handle (an
//!   overflow, a division by zero, too many nested calls) it gives up and the
//!   interpreter runs that call again, with the very same result or error;
//! - **loops**: any loop (`for`, `while`, `repeat`) that turns often, wherever it
//!   is, also in the main program. Besides numbers a loop handles array, list and
//!   matrix elements, `length`, `append` and global variables, through small Rust
//!   helpers. A compiled loop always ends the same way: it writes its registers
//!   back and returns the instruction the interpreter goes on with — the loop's
//!   exit, or, when it meets something it does not handle, that very instruction
//!   (which the interpreter then runs, with its own errors and messages).
//!
//! Every register `r` is held in four Cranelift variables: its value as an int, as
//! a real, as a bool, and a tag saying which of the three it holds now (or none:
//! the value is a list, a string... and stays in the interpreter's register). The
//! bytecode is typed, so a read as int always follows a write as int: typed
//! instructions use the typed variable directly; `Move`, calls and the way back to
//! the interpreter go through the tag.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, AbiParam, Block, InstBuilder, MemFlagsData, Signature, StackSlotData, StackSlotKind, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module as _};

use crate::bytecode::{Cmp, Function, Module, Op};
use crate::vm::{Packed, Value};

pub const TAG_NONE: i64 = 0;
pub const TAG_INT: i64 = 1;
pub const TAG_REAL: i64 = 2;
pub const TAG_BOOL: i64 = 3;
/// Only in the registers handed to compiled code: no value yet (Value::Null).
pub const TAG_NULL: i64 = 4;
/// returned as the tag of a function's result when its compiled code gives up
pub const TAG_BAIL: i64 = 255;

/// How many compiled calls may nest before the interpreter takes over (the
/// machine stack is small; the interpreter keeps its frames on the heap).
const NATIVE_DEPTH: i64 = 2000;
/// A loop is compiled after this many turns in the interpreter.
const HOT_EDGES: u32 = 50;
/// A function is compiled from its second call.
const HOT_CALLS: u32 = 2;
/// After this many give-ups a function is left to the interpreter (a deep
/// recursion or an error inside a `try` would otherwise make it start over and over).
const MAX_BAILS: u32 = 8;
/// After this many early exits (an instruction the loop does not handle, met
/// again and again) a loop is left to the interpreter.
const MAX_DEOPTS: u32 = 32;

const FAILED: u32 = u32::MAX;
const COMPILED: u32 = 1 << 31;

/// One register on its way in or out of compiled code.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Slot {
    pub bits: i64,
    pub tag: i64,
}

impl Slot {
    pub fn of(v: &Value) -> Slot {
        match v {
            Value::Int(n) => Slot { bits: *n, tag: TAG_INT },
            Value::Real(r) => Slot { bits: r.to_bits() as i64, tag: TAG_REAL },
            Value::Bool(b) => Slot { bits: *b as i64, tag: TAG_BOOL },
            _ => Slot { bits: 0, tag: TAG_NONE },
        }
    }

    /// As `of`, but "no value yet" has its own tag: compiled code may then hold
    /// a register of one kind with a constant tag (the language never reads a
    /// variable before giving it a value).
    pub fn entry(v: &Value) -> Slot {
        match v {
            Value::Null => Slot { bits: 0, tag: TAG_NULL },
            v => Slot::of(v),
        }
    }

    pub fn value(self) -> Option<Value> {
        match self.tag {
            TAG_INT => Some(Value::Int(self.bits)),
            TAG_REAL => Some(Value::Real(f64::from_bits(self.bits as u64))),
            TAG_BOOL => Some(Value::Bool(self.bits != 0)),
            _ => None,
        }
    }
}

/// Where a `Value` keeps its kind and its number, measured in this very binary.
/// Rust does not promise an enum's layout, so it is checked at start-up (a value
/// rebuilt from the measured layout must read back the same); when anything
/// differs, compiled loops reach elements through the helpers only.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    size: i64,
    t_null: u8,
    t_int: u8,
    t_real: u8,
    t_bool: u8,
}

fn probe_layout() -> Option<Layout> {
    use std::cell::RefCell;
    use std::mem::{size_of, MaybeUninit};
    use std::rc::Rc;
    let size = size_of::<Value>();
    if size < 16 {
        return None;
    }
    let byte0 = |v: &Value| unsafe { *(v as *const Value as *const u8) };
    let (t_null, t_int, t_real, t_bool) = (byte0(&Value::Null), byte0(&Value::Int(7)), byte0(&Value::Real(1.5)), byte0(&Value::Bool(true)));
    let scalar = [t_null, t_int, t_real, t_bool];
    let mut uniq = scalar.to_vec();
    uniq.sort_unstable();
    uniq.dedup();
    if uniq.len() != 4 {
        return None;
    }
    // values that own memory must never look like a scalar (writes skip their drop)
    let owners = [Value::Str("x".into()), Value::Seq(Rc::new(RefCell::new(Vec::new()))), Value::Func(3)];
    if owners.iter().any(|v| scalar.contains(&byte0(v))) {
        return None;
    }
    // numbers at offset 8, a bool at offset 1; rebuilt values must read back equal
    for x in [0i64, 1, -1, i64::MIN, i64::MAX, 0x1234_5678_9abc_def0] {
        let v = Value::Int(x);
        let at8 = unsafe { *((&v as *const Value as *const u8).add(8) as *const i64) };
        if at8 != x {
            return None;
        }
        let mut buf = MaybeUninit::<Value>::zeroed();
        unsafe {
            let p = buf.as_mut_ptr() as *mut u8;
            *p = t_int;
            *(p.add(8) as *mut i64) = x;
            if !matches!(buf.assume_init_ref(), Value::Int(y) if *y == x) {
                return None;
            }
        }
    }
    for x in [0.0f64, -2.5, 1e300, f64::INFINITY] {
        let mut buf = MaybeUninit::<Value>::zeroed();
        unsafe {
            let p = buf.as_mut_ptr() as *mut u8;
            *p = t_real;
            *(p.add(8) as *mut f64) = x;
            if !matches!(buf.assume_init_ref(), Value::Real(y) if y.to_bits() == x.to_bits()) {
                return None;
            }
        }
    }
    for x in [false, true] {
        let v = Value::Bool(x);
        if unsafe { *(&v as *const Value as *const u8).add(1) } != x as u8 {
            return None;
        }
        let mut buf = MaybeUninit::<Value>::zeroed();
        unsafe {
            let p = buf.as_mut_ptr() as *mut u8;
            *p = t_bool;
            *p.add(1) = x as u8;
            if !matches!(buf.assume_init_ref(), Value::Bool(y) if *y == x) {
                return None;
            }
        }
    }
    let mut buf = MaybeUninit::<Value>::zeroed();
    unsafe {
        *(buf.as_mut_ptr() as *mut u8) = t_null;
        if !matches!(buf.assume_init_ref(), Value::Null) {
            return None;
        }
    }
    Some(Layout { size: size as i64, t_null, t_int, t_real, t_bool })
}

/// What a compiled loop reaches of the interpreter: the current frame's registers
/// (for values that are not numbers) and the global variables.
#[repr(C)]
pub struct Ctx {
    pub regs: *mut Value,
    pub globals: *mut Value,
    /// the program's string constants
    pub strings: *const std::rc::Rc<str>,
    /// where `output` writes
    pub out: *mut dyn std::io::Write,
    /// the built-ins' state (windows, files, random numbers...)
    pub env: *mut crate::builtins::Env,
    /// an error a built-in raised: the interpreter reports it at the instruction
    /// compiled code stopped at, without running the built-in again
    pub error: Option<String>,
    /// the interpreter (for direct calls between compiled bodies)
    pub vm: *mut u8,
    /// the function running in this frame and its first register's index
    pub func: usize,
    pub base: usize,
}

/// Returned by a compiled body instead of an instruction: a function it called
/// stopped, so the interpreter goes on in that callee's frame (already pushed).
pub const CALLEE_ACTIVE: i64 = u32::MAX as i64;

/// `fn(args: *const Slot, budget: i64, out: *mut Slot)`
type Trampoline = unsafe extern "C" fn(*const Slot, i64, *mut Slot);
/// `fn(regs: *mut Slot, budget: i64, ctx: *mut Ctx, from: i64) -> i64`: runs a
/// compiled function body from instruction `from`; returns the instruction the
/// interpreter goes on with
type BodyFn = unsafe extern "C" fn(*mut Slot, i64, *mut Ctx, i64) -> i64;

/// A compiled function body: its entry point and the registers it reads and writes.
pub struct BodyCode {
    pub entry: BodyFn,
    pub used: Vec<u16>,
    pub written: Vec<u16>,
}

// ------------------------------------------------------------------ helpers
// Called from compiled loops; `ctx.regs` is the current frame's first register.
// Each returns 0, or 1 when the interpreter must run the instruction itself.

/// The value to store: from the bits when it is a number, else the register itself.
unsafe fn stored(ctx: *mut Ctx, bits: i64, tag: i64, src: i64) -> Value {
    match (Slot { bits, tag }).value() {
        Some(v) => v,
        None => (*(*ctx).regs.add(src as usize)).clone(),
    }
}

/// Puts an element's value in `out` (a number) or straight into register `d`.
unsafe fn give(ctx: *mut Ctx, d: i64, v: &Value, out: *mut Slot) -> i64 {
    if let Value::Null = v {
        return 1; // "this element has no value yet": the interpreter says it
    }
    let s = Slot::of(v);
    if s.tag == TAG_NONE {
        *(*ctx).regs.add(d as usize) = v.clone();
    }
    *out = s;
    0
}

unsafe extern "C" fn gw_get_elem(ctx: *mut Ctx, d: i64, c: i64, i: i64, out: *mut Slot) -> i64 {
    if let Value::Packed(p) = &*(*ctx).regs.add(c as usize) {
        let p = p.borrow();
        if i < 1 || i as u64 > p.len() as u64 {
            return 1;
        }
        let v = p.get(i as usize - 1);
        return give(ctx, d, &v, out);
    }
    let Value::Seq(q) = &*(*ctx).regs.add(c as usize) else { return 1 };
    let v = {
        let q = q.borrow();
        if i < 1 || i as u64 > q.len() as u64 {
            return 1;
        }
        q[i as usize - 1].clone()
    };
    give(ctx, d, &v, out)
}

unsafe extern "C" fn gw_set_elem(ctx: *mut Ctx, c: i64, i: i64, bits: i64, tag: i64, s: i64) -> i64 {
    let v = stored(ctx, bits, tag, s);
    if let Value::Packed(p) = &*(*ctx).regs.add(c as usize) {
        let mut p = p.borrow_mut();
        if i < 1 || i as u64 > p.len() as u64 {
            return 1;
        }
        p.set(i as usize - 1, &v);
        return 0;
    }
    let Value::Seq(q) = &*(*ctx).regs.add(c as usize) else { return 1 };
    let mut q = q.borrow_mut();
    if i < 1 || i as u64 > q.len() as u64 {
        return 1;
    }
    q[i as usize - 1] = v;
    0
}

unsafe extern "C" fn gw_get_elem2(ctx: *mut Ctx, d: i64, c: i64, i: i64, j: i64, out: *mut Slot) -> i64 {
    let Value::Matrix(mx) = &*(*ctx).regs.add(c as usize) else { return 1 };
    let v = {
        let mx = mx.borrow();
        if i < 1 || i as u64 > mx.rows as u64 || j < 1 || j as u64 > mx.cols as u64 {
            return 1;
        }
        mx.data[(i as usize - 1) * mx.cols + (j as usize - 1)].clone()
    };
    give(ctx, d, &v, out)
}

unsafe extern "C" fn gw_set_elem2(ctx: *mut Ctx, c: i64, i: i64, j: i64, bits: i64, tag: i64, s: i64) -> i64 {
    let v = stored(ctx, bits, tag, s);
    let Value::Matrix(mx) = &*(*ctx).regs.add(c as usize) else { return 1 };
    let mut mx = mx.borrow_mut();
    if i < 1 || i as u64 > mx.rows as u64 || j < 1 || j as u64 > mx.cols as u64 {
        return 1;
    }
    let cols = mx.cols;
    mx.data[(i as usize - 1) * cols + (j as usize - 1)] = v;
    0
}

/// The length, or -1 when the interpreter must work it out.
unsafe extern "C" fn gw_length(ctx: *mut Ctx, s: i64) -> i64 {
    match &*(*ctx).regs.add(s as usize) {
        Value::Seq(q) => q.borrow().len() as i64,
        Value::Packed(p) => p.borrow().len() as i64,
        Value::Dict(t) | Value::Set(t) => t.borrow().len() as i64,
        Value::Str(t) => t.chars().count() as i64,
        _ => -1,
    }
}

unsafe extern "C" fn gw_append(ctx: *mut Ctx, l: i64, bits: i64, tag: i64, s: i64) -> i64 {
    let v = stored(ctx, bits, tag, s);
    if let Value::Packed(p) = &*(*ctx).regs.add(l as usize) {
        p.borrow_mut().push(&v);
        return 0;
    }
    let Value::Seq(q) = &*(*ctx).regs.add(l as usize) else { return 1 };
    q.borrow_mut().push(v);
    0
}

unsafe extern "C" fn gw_get_global(ctx: *mut Ctx, g: i64, d: i64, out: *mut Slot) -> i64 {
    let v = (*(*ctx).globals.add(g as usize)).clone();
    give(ctx, d, &v, out)
}

unsafe extern "C" fn gw_set_global(ctx: *mut Ctx, g: i64, bits: i64, tag: i64, s: i64) -> i64 {
    let v = stored(ctx, bits, tag, s);
    *(*ctx).globals.add(g as usize) = v;
    0
}

/// `d = s` for a value that is not a number (it stays in the registers).
unsafe extern "C" fn gw_move(ctx: *mut Ctx, d: i64, s: i64) -> i64 {
    let v = (*(*ctx).regs.add(s as usize)).clone();
    *(*ctx).regs.add(d as usize) = v;
    0
}

/// An array or list's elements: `out` gets the address of the first, the count,
/// the kind (-1: values, 0: ints, 1: floats, 2: bools) and, for compact ints and
/// floats, the address of the "has a value" bits. 1 when it is no array or list.
unsafe extern "C" fn gw_seq_view(ctx: *mut Ctx, c: i64, out: *mut i64) -> i64 {
    match &*(*ctx).regs.add(c as usize) {
        Value::Seq(q) => {
            let v = &*q.as_ptr();
            *out = v.as_ptr() as i64;
            *out.add(1) = v.len() as i64;
            *out.add(2) = -1;
            0
        }
        Value::Packed(p) => {
            let (data, n, kind, bits) = match &*p.as_ptr() {
                Packed::Ints(v, b) => (v.as_ptr() as i64, v.len(), 0, if b.is_empty() { 0 } else { b.as_ptr() as i64 }),
                Packed::Reals(v, b) => (v.as_ptr() as i64, v.len(), 1, if b.is_empty() { 0 } else { b.as_ptr() as i64 }),
                Packed::Bools(v) => (v.as_ptr() as i64, v.len(), 2, 0),
            };
            *out = data;
            *out.add(1) = n as i64;
            *out.add(2) = kind;
            *out.add(3) = bits;
            0
        }
        _ => 1,
    }
}

/// Compact array in register `c`: drop its "has a value" bits if every element
/// has one. Returns the bits' address, 0 when there are none (full) or no such array.
unsafe extern "C" fn gw_try_full(ctx: *mut Ctx, c: i64) -> i64 {
    if let Value::Packed(p) = &*(*ctx).regs.add(c as usize) {
        let pk = &mut *p.as_ptr();
        if pk.try_full() {
            return 0;
        }
        if let Packed::Ints(_, b) | Packed::Reals(_, b) = pk {
            return b.as_ptr() as i64;
        }
    }
    0
}

/// A matrix's cells: address of the first, rows, columns.
unsafe extern "C" fn gw_mat_view(ctx: *mut Ctx, c: i64, out: *mut i64) -> i64 {
    let Value::Matrix(mx) = &*(*ctx).regs.add(c as usize) else { return 1 };
    let mx = &*mx.as_ptr();
    *out = mx.data.as_ptr() as i64;
    *out.add(1) = mx.rows as i64;
    *out.add(2) = mx.cols as i64;
    0
}

/// The value of a register as text, from its bits when it is a number.
unsafe fn value_of(ctx: *mut Ctx, bits: i64, tag: i64, r: i64) -> Value {
    stored(ctx, bits, tag, r)
}

unsafe extern "C" fn gw_load_str(ctx: *mut Ctx, d: i64, k: i64) -> i64 {
    *(*ctx).regs.add(d as usize) = Value::Str((*(*ctx).strings.add(k as usize)).clone());
    0
}

unsafe extern "C" fn gw_concat(ctx: *mut Ctx, d: i64, a: i64, b: i64) -> i64 {
    let (Value::Str(x), Value::Str(y)) = (&*(*ctx).regs.add(a as usize), &*(*ctx).regs.add(b as usize)) else { return 1 };
    let mut t = String::with_capacity(x.len() + y.len());
    t.push_str(x);
    t.push_str(y);
    *(*ctx).regs.add(d as usize) = Value::Str(t.into());
    0
}

unsafe extern "C" fn gw_to_str(ctx: *mut Ctx, d: i64, bits: i64, tag: i64, s: i64) -> i64 {
    let t = crate::vm::display(&value_of(ctx, bits, tag, s));
    *(*ctx).regs.add(d as usize) = Value::Str(t.into());
    0
}

/// 0 or 1: the comparison of two strings; 2 when they are not both strings.
unsafe extern "C" fn gw_cmp_str(ctx: *mut Ctx, c: i64, a: i64, b: i64) -> i64 {
    let (Value::Str(x), Value::Str(y)) = (&*(*ctx).regs.add(a as usize), &*(*ctx).regs.add(b as usize)) else { return 2 };
    let (x, y): (&str, &str) = (x, y);
    (match c {
        0 => x == y,
        1 => x != y,
        2 => x < y,
        3 => x <= y,
        4 => x > y,
        _ => x >= y,
    }) as i64
}

unsafe extern "C" fn gw_char_at(ctx: *mut Ctx, d: i64, s: i64, i: i64) -> i64 {
    let Value::Str(t) = &*(*ctx).regs.add(s as usize) else { return 1 };
    if i < 1 {
        return 1;
    }
    match t.chars().nth(i as usize - 1) {
        Some(ch) => {
            *(*ctx).regs.add(d as usize) = Value::Str(ch.to_string().into());
            0
        }
        None => 1,
    }
}

unsafe extern "C" fn gw_print(ctx: *mut Ctx, bits: i64, tag: i64, s: i64) -> i64 {
    let t = crate::vm::display(&value_of(ctx, bits, tag, s));
    let _ = (*(*ctx).out).write_all(t.as_bytes());
    0
}

unsafe extern "C" fn gw_println(ctx: *mut Ctx) -> i64 {
    let _ = (*(*ctx).out).write_all(b"\n");
    0
}

unsafe extern "C" fn gw_new_list(ctx: *mut Ctx, d: i64) -> i64 {
    *(*ctx).regs.add(d as usize) = Value::Seq(std::rc::Rc::new(std::cell::RefCell::new(Vec::new())));
    0
}

/// Where the interpreter may go into a compiled body: the start, the heads of
/// loops, after a call, a handler's catch and after an instruction the body
/// leaves to the interpreter. Elsewhere the interpreter goes on until one of
/// these: fewer entries leave the compiler fewer join points, so faster code.
fn entries(code: &[Op], runs: &[bool]) -> Vec<bool> {
    let n = code.len();
    let mut e = vec![false; n];
    if n > 0 {
        e[0] = true;
    }
    let mut mark = |t: usize| {
        if t < n {
            e[t] = true;
        }
    };
    for (ip, op) in code.iter().enumerate() {
        match *op {
            Op::Jump(t) | Op::JumpIfFalse(_, t) | Op::JumpIfTrue(_, t) | Op::JumpUnlessI(_, _, _, t) | Op::JumpUnlessIImm(_, _, _, t) if (t as usize) <= ip => mark(t as usize),
            Op::ForStep { body, .. } => mark(body as usize),
            Op::PushHandler(c, _) => mark(c as usize),
            Op::Call { .. } | Op::CallValue { .. } | Op::Builtin { .. } => mark(ip + 1),
            _ => {}
        }
        if !runs[ip] {
            mark(ip + 1);
        }
    }
    e
}

/// Calls built-in `id` with `argc` arguments (their bits and tags at `args`; a
/// value that is not a number is read from register `first + k`). A function's
/// result goes to `out` (a number) or straight into register `dst`.
unsafe extern "C" fn gw_builtin(ctx: *mut Ctx, id: i64, first: i64, argc: i64, dst: i64, args: *const Slot, out: *mut Slot) -> i64 {
    let vals: Vec<Value> = (0..argc as usize)
        .map(|k| {
            let a = *args.add(k);
            match a.value() {
                Some(v) => v,
                None => (*(*ctx).regs.add(first as usize + k)).clone(),
            }
        })
        .collect();
    let b = crate::builtins::Builtin::from_id(id as u16);
    match (*(*ctx).env).call(b, &vals) {
        Ok(v) => {
            if b.is_function() {
                let sl = Slot::of(&v);
                if sl.tag == TAG_NONE {
                    *(*ctx).regs.add(dst as usize) = v;
                }
                *out = sl;
            }
            0
        }
        Err(msg) => {
            (*ctx).error = Some(msg);
            1
        }
    }
}

/// Built-ins compiled code may call: all but those that need the interpreter
/// itself (tasks, dialogs that flush the output first, stop).
pub fn callable_builtin(id: u16) -> bool {
    use crate::builtins::Builtin as B;
    let b = B::from_id(id);
    !matches!(
        b,
        B::TaskSpawn | B::TaskYield | B::TaskAwait | B::TaskDone | B::TaskId | B::TaskCount | B::TaskChannel | B::TaskSend | B::TaskRecv | B::TaskHas
            | B::Message | B::Confirm | B::Ask | B::Stop
    )
}

/// `fn(lo, hi, buf, out) -> status`: one piece of a parallel loop, its counter
/// from `lo` to `hi`; per accumulator it writes the sum, and the lowest and
/// highest running sums, to `out`. Status 0: done; else something it cannot do.
type ChunkFn = unsafe extern "C" fn(i64, i64, *const i64, *mut i64) -> i64;

/// A loop compiled to run in pieces.
pub struct ParLoop {
    chunk: ChunkFn,
    accs: Vec<u16>,
    /// compact arrays written (at the counter's position) and read
    writes: Vec<u16>,
    reads: Vec<u16>,
    /// pieces start at multiples of this element (64: the "has a value" bits
    /// of written int or float arrays are shared by 64 elements)
    align: i64,
    /// fewer turns than this: not worth the threads
    min_trip: i64,
}

/// An innermost for loop with a second, check-free copy of its body.
struct FastLoop {
    fs: usize,
    var: u16,
    end: u16,
    step: u16,
    /// compact arrays read or written at the counter's position
    conts: Vec<u16>,
    /// the block that chooses the copy, and the copy's blocks (head..=fs)
    choose: Block,
    blocks: Vec<Block>,
}

/// Innermost for loops whose counter indexes compact arrays: (ForInit, ForStep,
/// var, end, step, those arrays). The counter and the end are not written in the
/// body, and the arrays' views stay valid there (fresh, not replaced).
#[allow(clippy::type_complexity)]
fn fast_loops(code: &[Op], spec: &[i8], fresh: &[bool], clob: &[bool]) -> Vec<(usize, usize, u16, u16, u16, Vec<u16>)> {
    let mut out = Vec::new();
    for fi in 0..code.len() {
        let Op::ForInit { var, end, step, exit } = code[fi] else { continue };
        let exit = exit as usize;
        if exit == 0 || exit > code.len() {
            continue;
        }
        let fs = exit - 1;
        match code[fs] {
            Op::ForStep { var: v2, end: e2, step: s2, body } if v2 == var && e2 == end && s2 == step && body as usize == fi + 1 => {}
            _ => continue,
        }
        let body = fi + 1..fs;
        if code[body.clone()].iter().any(|op| matches!(op, Op::ForInit { .. })) {
            continue; // innermost loops only
        }
        if code[body.clone()].iter().any(|op| written_reg(op).is_some_and(|w| w == var || w == end || w == step)) {
            continue;
        }
        let mut conts: Vec<u16> = Vec::new();
        let mut bad = false;
        for ip in body.clone() {
            if let Op::GetElem(_, c, i) | Op::SetElem(c, i, _) = code[ip] {
                if i == var && spec.get(c as usize).is_some_and(|&k| k != SPEC_NONE) {
                    if fresh.get(ip).copied().unwrap_or(false) && !clob.get(c as usize).copied().unwrap_or(true) {
                        conts.push(c);
                    } else {
                        bad = true;
                    }
                }
            }
        }
        if bad || conts.is_empty() {
            continue;
        }
        if code[body.clone()].iter().any(|op| written_reg(op).is_some_and(|w| conts.contains(&w))) {
            continue;
        }
        conts.sort_unstable();
        conts.dedup();
        out.push((fi, fs, var, end, step, conts));
    }
    out
}

/// What a compiled body needs to run a loop in parallel.
struct ParHook {
    info: i64,
    accs: Vec<u16>,
    inputs: Vec<u16>,
    var: u16,
    end: u16,
    step: u16,
    exit: usize,
    n_regs: usize,
}

/// Registers in the buffer of a parallel loop: bits, tag, and a compact
/// array's address, length and "has a value" bits.
const PAR_SLOT: usize = 5;

/// Runs a reduction loop's turns `lo..=hi` in pieces on every core. The
/// accumulators' sums are added in order, checking that no running sum of the
/// sequential loop would have overflowed; anything a piece cannot do, or an
/// overflow, gives 1 and the loop runs sequentially (with the interpreter's
/// errors). 2: not worth it. 0: done, the accumulators' values in `out`.
unsafe extern "C" fn gw_par(ctx: *mut Ctx, info: i64, lo: i64, hi: i64, buf: *const i64, out: *mut i64) -> i64 {
    use rayon::prelude::*;
    let p = &*(info as *const ParLoop);
    if hi == i64::MAX || hi < lo {
        return 2;
    }
    if !p.writes.is_empty() {
        // writing: an error would leave later elements written, which only an
        // ended program may do (no try around, no other task); and no array
        // written may be one read
        if !crate::vm::jit_errors_end_program((*ctx).vm) {
            return 2;
        }
        let addr = |r: u16| *buf.add(r as usize * PAR_SLOT + 2);
        for &w in &p.writes {
            if p.reads.iter().any(|&r| addr(r) == addr(w)) {
                return 2;
            }
        }
    }
    let trip = hi as i128 - lo as i128 + 1;
    let threads = rayon::current_num_threads() as i128;
    if trip < p.min_trip as i128 || threads < 2 {
        return 2;
    }
    for &a in &p.accs {
        if *buf.add(a as usize * PAR_SLOT + 1) != TAG_INT {
            return 2;
        }
    }
    let pieces = (threads * 8).min(trip);
    let na = p.accs.len();
    let bufu = buf as usize;
    let chunk = p.chunk;
    // piece boundaries (counter values), aligned: element = counter - 1
    let mut bounds: Vec<i128> = Vec::with_capacity(pieces as usize + 1);
    for k in 0..=pieces {
        let mut b = lo as i128 + trip * k / pieces;
        if k > 0 && k < pieces && p.align > 1 {
            let al = p.align as i128;
            b = ((b - 1 + al - 1).div_euclid(al)) * al + 1;
            b = b.min(hi as i128 + 1);
        }
        if bounds.last().is_none_or(|&l| b > l) || k == 0 {
            bounds.push(b);
        }
    }
    if *bounds.last().unwrap() != hi as i128 + 1 {
        bounds.push(hi as i128 + 1);
    }
    let res: Vec<(i64, Vec<i64>)> = (0..bounds.len() - 1)
        .into_par_iter()
        .map(|k| {
            let a = bounds[k];
            let b = bounds[k + 1] - 1;
            let mut o = vec![0i64; 3 * na];
            // SAFETY: the piece reads the buffer and arrays only, writes `o`
            let st = unsafe { chunk(a as i64, b as i64, bufu as *const i64, o.as_mut_ptr()) };
            (st, o)
        })
        .collect();
    if res.iter().any(|(st, _)| *st != 0) {
        return 1;
    }
    for (k, &a) in p.accs.iter().enumerate() {
        let mut acc = *buf.add(a as usize * PAR_SLOT) as i128;
        for (_, o) in &res {
            let (sum, lo_run, hi_run) = (o[3 * k] as i128, o[3 * k + 1] as i128, o[3 * k + 2] as i128);
            if acc + lo_run < i64::MIN as i128 || acc + hi_run > i64::MAX as i128 {
                return 1;
            }
            acc += sum;
        }
        *out.add(k) = acc as i64;
    }
    0
}

/// A for loop that can run in pieces: from ForInit at `fi` to its ForStep at `fs`.
struct ParPlan {
    fi: usize,
    fs: usize,
    exit: usize,
    var: u16,
    end: u16,
    step: u16,
    accs: Vec<u16>,
    writes: Vec<u16>,
    reads: Vec<u16>,
    align: i64,
    /// (instruction, accumulator index, its operand holding the accumulator's value)
    affine: Vec<(usize, usize, u16)>,
    inputs: Vec<u16>,
    nested: bool,
}

/// Follows accumulator `a` through the body of the loop (fi+1..fs): a value
/// "derived" from it is `a` plus or minus a value that is not. Every instruction
/// reading a possibly derived value must be such an add, subtract or move of a
/// surely derived one, and every write to `a` must be. Gives the instructions
/// (with their derived operand) and, per instruction, the possibly derived registers.
#[allow(clippy::type_complexity)]
fn affine_acc(code: &[Op], fi: usize, fs: usize, a: u16) -> Option<(Vec<(usize, u16)>, Vec<Vec<bool>>)> {
    let lo = fi + 1;
    let len = fs - lo;
    let nr = 1 + code[lo..fs].iter().flat_map(|op| {
        let mut l = Vec::new();
        regs_of(op, &mut l);
        l
    }).chain([a]).max().unwrap_or(0) as usize;
    // per instruction: (possibly derived, surely derived) on the way in; None: not reached yet
    let mut state: Vec<Option<(Vec<bool>, Vec<bool>)>> = vec![None; len];
    let mut head = vec![false; nr];
    head[a as usize] = true;
    state[0] = Some((head.clone(), head));
    let mut ops: Vec<(usize, u16)> = Vec::new();
    let mut changed = true;
    let mut rounds = 0;
    while changed {
        changed = false;
        rounds += 1;
        if rounds > 64 {
            return None;
        }
        ops.clear();
        for ip in lo..fs {
            let Some((may, must)) = state[ip - lo].clone() else { continue };
            let op = code[ip];
            let mut reads = Vec::new();
            regs_of(&op, &mut reads);
            let dest = written_reg(&op);
            if let Some(d) = dest {
                if let Some(p) = reads.iter().position(|&r| r == d) {
                    reads.remove(p);
                }
            }
            let touched = reads.iter().any(|&r| may[r as usize]);
            let (mut may2, mut must2) = (may.clone(), must.clone());
            if touched {
                // an add, subtract or move of exactly one surely derived operand
                let der = match op {
                    Op::AddI(_, x, y) => {
                        if must[x as usize] && !may[y as usize] {
                            Some(x)
                        } else if must[y as usize] && !may[x as usize] {
                            Some(y)
                        } else {
                            None
                        }
                    }
                    Op::SubI(_, x, y) if must[x as usize] && !may[y as usize] => Some(x),
                    Op::AddIImm(_, x, _) | Op::SubIImm(_, x, _) | Op::Move(_, x) if must[x as usize] => Some(x),
                    _ => None,
                };
                let der = der?;
                let d = dest?;
                may2[d as usize] = true;
                must2[d as usize] = true;
                ops.push((ip, der));
            } else if let Some(d) = dest {
                if d == a {
                    return None; // the accumulator overwritten
                }
                may2[d as usize] = false;
                must2[d as usize] = false;
            } else if matches!(op, Op::Call { .. } | Op::Builtin { .. }) {
                return None;
            }
            for t in successors(code, ip).into_iter().flatten() {
                if t < lo || t >= fs {
                    continue; // the end of the turn (fs) or out of the loop
                }
                let slot = &mut state[t - lo];
                match slot {
                    None => {
                        *slot = Some((may2.clone(), must2.clone()));
                        changed = true;
                    }
                    Some((m, u)) => {
                        for r in 0..nr {
                            let nm = m[r] || may2[r];
                            let nu = u[r] && must2[r];
                            if nm != m[r] || nu != u[r] {
                                m[r] = nm;
                                u[r] = nu;
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }
    // the accumulator surely derived at the end of the turn: every way into fs
    for ip in lo..fs {
        if successors(code, ip).into_iter().flatten().any(|t| t == fs) {
            let (may, must) = state[ip - lo].clone()?;
            // the state after ip
            let op = code[ip];
            let wrote_a = written_reg(&op) == Some(a);
            let derived_after = if wrote_a { ops.iter().any(|&(i, _)| i == ip) } else { must[a as usize] };
            if !derived_after {
                return None;
            }
            let _ = may;
        }
    }
    let may: Vec<Vec<bool>> = state.into_iter().map(|s| s.map(|(m, _)| m).unwrap_or_else(|| vec![false; nr])).collect();
    Some((ops, may))
}

/// The for loops whose turns are independent but for integer sums: only numbers,
/// compact-array reads and jumps inside the loop; every register written in it is
/// an accumulator (only ever `a = a + x`) or used only within a turn.
fn par_plans(code: &[Op], runs: &[bool], live: &[Vec<u64>], masks: &[u8], spec: &[i8], fresh: &[bool], clob: &[bool]) -> Vec<ParPlan> {
    let is_live = |ip: usize, r: u16| live.get(ip).is_some_and(|l| l[r as usize / 64] >> (r % 64) & 1 == 1);
    let mut plans: Vec<ParPlan> = Vec::new();
    for fi in 0..code.len() {
        let Op::ForInit { var, end, step, exit } = code[fi] else { continue };
        let exit = exit as usize;
        let dbg = std::env::var_os("GLOBW_JIT_PARDBG").is_some();
        if exit == 0 || exit > code.len() || !runs[fi] || !fresh.get(fi).copied().unwrap_or(false) {
            if dbg { eprintln!("par {}: entry/fresh", fi); }
            continue;
        }
        let fs = exit - 1;
        match code[fs] {
            Op::ForStep { var: v2, end: e2, step: s2, body } if v2 == var && e2 == end && s2 == step && body as usize == fi + 1 => {}
            _ => continue,
        }
        if plans.last().is_some_and(|p| fi < p.fs) {
            continue; // inside a loop already run in parallel
        }
        let in_r = |t: usize| t > fi && t <= fs;
        let mut ok = fs > fi + 1;
        let mut nested = false;
        let usable = |c: u16| spec.get(c as usize).is_some_and(|&k| k != SPEC_NONE) && !clob.get(c as usize).copied().unwrap_or(true);
        let (mut writes, mut reads, mut align): (Vec<u16>, Vec<u16>, i64) = (Vec::new(), Vec::new(), 1);
        for ip in fi + 1..fs {
            if !runs[ip] {
                ok = false;
                break;
            }
            let good = match code[ip] {
                Op::GetElem(_, c, _) => {
                    reads.push(c);
                    usable(c)
                }
                // a write at the counter's own position: each turn its own element
                Op::SetElem(c, i, _) => {
                    writes.push(c);
                    if spec.get(c as usize).is_some_and(|&k| k == 0 || k == 1) {
                        align = 64;
                    }
                    usable(c) && i == var
                }
                Op::Jump(t) | Op::JumpIfFalse(_, t) | Op::JumpIfTrue(_, t) | Op::JumpUnlessI(_, _, _, t) | Op::JumpUnlessIImm(_, _, _, t) => in_r(t as usize),
                Op::ForInit { exit: x, .. } => {
                    nested = true;
                    in_r(x as usize)
                }
                Op::ForStep { body, .. } => in_r(body as usize),
                op => plain_numeric(&op),
            };
            if !good {
                ok = false;
                break;
            }
        }
        if !ok {
            if dbg { eprintln!("par {}: ops", fi); }
            continue;
        }
        let mut written: Vec<u16> = (fi + 1..fs).filter_map(|ip| written_reg(&code[ip])).collect();
        written.sort_unstable();
        written.dedup();
        if written.contains(&var) {
            continue;
        }
        // accumulators: registers that only ever hold "the accumulator plus or
        // minus something that does not depend on it", through temporaries
        let mut accs = Vec::new();
        let mut affine: Vec<(usize, usize, u16)> = Vec::new();
        let mut may_all: Vec<Vec<Vec<bool>>> = Vec::new();
        for &a in &written {
            if masks.get(a as usize).map_or(true, |m| m & 7 != K_INT) || !is_live(fi + 1, a) {
                continue;
            }
            if let Some((ops, may)) = affine_acc(code, fi, fs, a) {
                accs.push(a);
                let k = accs.len() - 1;
                affine.extend(ops.into_iter().map(|(ip, der)| (ip, k, der)));
                may_all.push(may);
            }
        }
        // an operation affine for one accumulator may not mix in another one
        let mut clash = false;
        for &(ip, k, der) in &affine {
            if affine.iter().any(|&(ip2, k2, _)| ip2 == ip && k2 != k) {
                clash = true;
            }
            let mut l = Vec::new();
            regs_of(&code[ip], &mut l);
            for (k2, may) in may_all.iter().enumerate() {
                if k2 != k && l.iter().any(|&r| r != der && may[ip - fi - 1].get(r as usize).copied().unwrap_or(false)) {
                    clash = true;
                }
            }
        }
        if clash {
            if dbg { eprintln!("par {}: accumulators mixed", fi); }
            continue;
        }
        writes.sort_unstable();
        writes.dedup();
        reads.sort_unstable();
        reads.dedup();
        // a written array is not read in the loop (a turn would see another's writes)
        if writes.iter().any(|w| reads.contains(w)) {
            if dbg { eprintln!("par {}: reads what it writes", fi); }
            continue;
        }
        if accs.is_empty() && writes.is_empty() {
            if dbg { eprintln!("par {}: no accumulator (written {:?}, masks {:?})", fi, written, written.iter().map(|&r| masks[r as usize]).collect::<Vec<_>>()); }
            continue;
        }
        // every other register written: used within a turn only, dead after the loop
        if written.iter().any(|&r| !accs.contains(&r) && (is_live(fi + 1, r) || is_live(exit, r))) {
            if dbg { eprintln!("par {}: carried", fi); }
            continue;
        }
        let mut inputs = Vec::new();
        for ip in fi + 1..fs {
            regs_of(&code[ip], &mut inputs);
        }
        inputs.extend(&accs);
        inputs.sort_unstable();
        inputs.dedup();
        inputs.retain(|&r| r != var && (accs.contains(&r) || !written.contains(&r)));
        plans.push(ParPlan { fi, fs, exit, var, end, step, accs, writes, reads, align, affine, inputs, nested });
    }
    plans
}

const HELPERS: [(&str, usize, bool); 23] = [
    // name, number of i64 arguments after ctx, ends with an `out` pointer
    ("gw_get_elem", 3, true),
    ("gw_set_elem", 5, false),
    ("gw_get_elem2", 4, true),
    ("gw_set_elem2", 6, false),
    ("gw_length", 1, false),
    ("gw_append", 4, false),
    ("gw_get_global", 2, true),
    ("gw_set_global", 4, false),
    ("gw_move", 2, false),
    ("gw_seq_view", 1, true),
    ("gw_mat_view", 1, true),
    ("gw_load_str", 2, false),
    ("gw_concat", 3, false),
    ("gw_to_str", 4, false),
    ("gw_cmp_str", 3, false),
    ("gw_char_at", 3, false),
    ("gw_print", 3, false),
    ("gw_println", 0, false),
    ("gw_new_list", 1, false),
    ("gw_builtin", 4, true),
    ("gw_call_body", 5, true),
    ("gw_par", 3, true),
    ("gw_try_full", 1, false),
];

fn helper_ptr(name: &str) -> *const u8 {
    match name {
        "gw_get_elem" => gw_get_elem as *const u8,
        "gw_set_elem" => gw_set_elem as *const u8,
        "gw_get_elem2" => gw_get_elem2 as *const u8,
        "gw_set_elem2" => gw_set_elem2 as *const u8,
        "gw_length" => gw_length as *const u8,
        "gw_append" => gw_append as *const u8,
        "gw_get_global" => gw_get_global as *const u8,
        "gw_set_global" => gw_set_global as *const u8,
        "gw_seq_view" => gw_seq_view as *const u8,
        "gw_load_str" => gw_load_str as *const u8,
        "gw_concat" => gw_concat as *const u8,
        "gw_to_str" => gw_to_str as *const u8,
        "gw_cmp_str" => gw_cmp_str as *const u8,
        "gw_char_at" => gw_char_at as *const u8,
        "gw_print" => gw_print as *const u8,
        "gw_println" => gw_println as *const u8,
        "gw_new_list" => gw_new_list as *const u8,
        "gw_builtin" => gw_builtin as *const u8,
        "gw_mat_view" => gw_mat_view as *const u8,
        "gw_par" => gw_par as *const u8,
        "gw_try_full" => gw_try_full as *const u8,
        _ => gw_move as *const u8,
    }
}

pub struct Jit {
    /// GLOBW_JIT_LOG: report what is compiled and every give-up (for development)
    log: bool,
    module: JITModule,
    ctx: Context,
    fctx: FunctionBuilderContext,
    helpers: HashMap<&'static str, FuncId>,
    /// how a Value lies in memory, when it could be checked
    layout: Option<Layout>,
    /// functions that can be compiled (all their instructions are handled)
    eligible: Vec<bool>,
    /// once the eligible functions are compiled: typed entry and trampoline
    funcs: Vec<Option<(FuncId, Trampoline)>>,
    compiled_funcs: bool,
    calls: Vec<u32>,
    func_bails: Vec<u32>,
    /// per function: how hot it is (calls and loop turns)
    heat: Vec<u32>,
    /// per function: not tried yet (0), FAILED, or COMPILED + index into `bodies`
    state: Vec<u32>,
    bodies: Vec<BodyCode>,
    /// per compiled function, per instruction: can the compiled body run it
    runs: Vec<Vec<bool>>,
    /// per compiled function, per instruction: may the interpreter enter there
    enter: Vec<Vec<bool>>,
    /// loops compiled to run in parallel (their address is in compiled code)
    par: Vec<Box<ParLoop>>,
    /// per compiled function, per instruction: times compiled code stopped there
    stops: Vec<Vec<u16>>,
    next_name: u32,
}

/// Instructions a compiled function handles (`Call` is checked separately).
fn plain_numeric(op: &Op) -> bool {
    matches!(
        op,
        Op::LoadInt(..)
            | Op::LoadReal(..)
            | Op::LoadBool(..)
            | Op::Move(..)
            | Op::AddI(..)
            | Op::SubI(..)
            | Op::MulI(..)
            | Op::DivI(..)
            | Op::ModI(..)
            | Op::AddR(..)
            | Op::SubR(..)
            | Op::MulR(..)
            | Op::DivR(..)
            | Op::NegI(..)
            | Op::NegR(..)
            | Op::Not(..)
            | Op::ToReal(..)
            | Op::CmpI(..)
            | Op::CmpR(..)
            | Op::CmpB(..)
            | Op::Jump(..)
            | Op::JumpIfFalse(..)
            | Op::JumpIfTrue(..)
            | Op::JumpUnlessI(..)
            | Op::JumpUnlessIImm(..)
            | Op::AddIImm(..)
            | Op::SubIImm(..)
            | Op::MulIImm(..)
            | Op::DivIImm(..)
            | Op::ModIImm(..)
            | Op::CmpIImm(..)
            | Op::ForInit { .. }
            | Op::ForStep { .. }
    )
}

/// Instructions a compiled loop handles besides the numeric ones and calls.
fn loop_extra(op: &Op) -> bool {
    matches!(
        op,
        Op::GetElem(..) | Op::SetElem(..) | Op::GetElem2(..) | Op::SetElem2(..) | Op::Length(..) | Op::Append(..) | Op::GetGlobal(..) | Op::SetGlobal(..)
            | Op::LoadStr(..) | Op::Concat(..) | Op::ToStr(..) | Op::CmpS(..) | Op::CharAt(..) | Op::Print(..) | Op::PrintLn | Op::NewList(..)
    )
}

/// The registers an instruction touches.
fn regs_of(op: &Op, out: &mut Vec<u16>) {
    match *op {
        Op::LoadInt(d, _) | Op::LoadReal(d, _) | Op::LoadBool(d, _) => out.push(d),
        Op::Move(d, s) | Op::NegI(d, s) | Op::NegR(d, s) | Op::Not(d, s) | Op::ToReal(d, s) | Op::Length(d, s) => out.extend([d, s]),
        Op::AddI(d, a, b) | Op::SubI(d, a, b) | Op::MulI(d, a, b) | Op::DivI(d, a, b) | Op::ModI(d, a, b) => out.extend([d, a, b]),
        Op::AddR(d, a, b) | Op::SubR(d, a, b) | Op::MulR(d, a, b) | Op::DivR(d, a, b) => out.extend([d, a, b]),
        Op::CmpI(_, d, a, b) | Op::CmpR(_, d, a, b) | Op::CmpB(_, d, a, b) => out.extend([d, a, b]),
        Op::JumpIfFalse(c, _) | Op::JumpIfTrue(c, _) => out.push(c),
        Op::JumpUnlessI(_, a, b, _) => out.extend([a, b]),
        Op::JumpUnlessIImm(_, a, _, _) => out.push(a),
        Op::AddIImm(d, a, _) | Op::SubIImm(d, a, _) | Op::MulIImm(d, a, _) | Op::DivIImm(d, a, _) | Op::ModIImm(d, a, _) => out.extend([d, a]),
        Op::CmpIImm(_, d, a, _) => out.extend([d, a]),
        Op::ForInit { var, end, step, .. } | Op::ForStep { var, end, step, .. } => out.extend([var, end, step]),
        Op::Call { first, argc, dst, .. } => {
            out.extend(first..first + argc);
            out.push(dst);
        }
        Op::Ret(s) => out.push(s),
        Op::GetElem(d, c, i) | Op::SetElem(c, i, d) => out.extend([d, c, i]),
        Op::GetElem2(d, c, i, j) | Op::SetElem2(c, i, j, d) => out.extend([d, c, i, j]),
        Op::Append(l, s) => out.extend([l, s]),
        Op::GetGlobal(d, _) | Op::SetGlobal(_, d) => out.push(d),
        Op::LoadStr(d, _) | Op::NewList(d) | Op::Print(d) => out.push(d),
        Op::Builtin { first, argc, dst, .. } => {
            out.extend(first..first + argc);
            out.push(dst);
        }
        Op::Concat(d, a, b) | Op::CmpS(_, d, a, b) | Op::CharAt(d, a, b) => out.extend([d, a, b]),
        Op::ToStr(d, s) => out.extend([d, s]),
        _ => {}
    }
}

/// Per instruction, the registers read from there on before being written (a
/// bit set), or None when that cannot be told: a handler may jump anywhere, or
/// the function is too big. Instructions compiled code does not run count as
/// reading every register.
fn live_in(code: &[Op], runs: &[bool], n_regs: usize) -> Option<Vec<Vec<u64>>> {
    let n = code.len();
    let words = n_regs.div_ceil(64).max(1);
    if code.iter().any(|op| matches!(op, Op::PushHandler(..))) || n * words > 1 << 22 {
        return None;
    }
    let all: Vec<u64> = {
        let mut v = vec![u64::MAX; words];
        let extra = words * 64 - n_regs;
        if extra > 0 {
            v[words - 1] >>= extra;
        }
        v
    };
    // per instruction: what it reads and the one register it surely writes
    let mut reads: Vec<Vec<u64>> = Vec::with_capacity(n);
    let mut kills: Vec<Option<u16>> = Vec::with_capacity(n);
    let mut list = Vec::new();
    for (ip, op) in code.iter().enumerate() {
        // a return reads at most its value, wherever it runs
        if let Op::Ret(_) | Op::RetNone = op {
            let mut r = vec![0u64; words];
            if let Op::Ret(x) = *op {
                r[x as usize / 64] |= 1 << (x % 64);
            }
            reads.push(r);
            kills.push(None);
            continue;
        }
        if !runs[ip] {
            reads.push(all.clone());
            kills.push(None);
            continue;
        }
        list.clear();
        regs_of(op, &mut list);
        // calls and built-ins may leave their destination alone; a for step reads its counter
        let kill = match op {
            Op::Call { .. } | Op::Builtin { .. } | Op::ForStep { .. } => None,
            op => written_reg(op),
        };
        if let Some(d) = kill {
            if let Some(p) = list.iter().position(|&r| r == d) {
                list.remove(p);
            }
        }
        let mut r = vec![0u64; words];
        for &x in &list {
            r[x as usize / 64] |= 1 << (x % 64);
        }
        reads.push(r);
        kills.push(kill);
    }
    let succ = |ip: usize| successors(code, ip);
    let mut live = vec![vec![0u64; words]; n];
    let mut changed = true;
    let mut out = vec![0u64; words];
    while changed {
        changed = false;
        for ip in (0..n).rev() {
            out.iter_mut().for_each(|w| *w = 0);
            for t in succ(ip).into_iter().flatten() {
                if t < n {
                    for (o, l) in out.iter_mut().zip(&live[t]) {
                        *o |= *l;
                    }
                }
            }
            if let Some(d) = kills[ip] {
                out[d as usize / 64] &= !(1 << (d % 64));
            }
            for (o, r) in out.iter_mut().zip(&reads[ip]) {
                *o |= *r;
            }
            if out != live[ip] {
                live[ip].copy_from_slice(&out);
                changed = true;
            }
        }
    }
    Some(live)
}

/// Where control may go after instruction `ip`.
fn successors(code: &[Op], ip: usize) -> [Option<usize>; 2] {
    let n = code.len();
    let next = if ip + 1 < n { Some(ip + 1) } else { None };
    match code[ip] {
        Op::Jump(t) => [Some(t as usize), None],
        Op::JumpIfFalse(_, t) | Op::JumpIfTrue(_, t) | Op::JumpUnlessI(_, _, _, t) | Op::JumpUnlessIImm(_, _, _, t) => [next, Some(t as usize)],
        Op::ForInit { exit, .. } => [next, Some(exit as usize)],
        Op::ForStep { body, .. } => [Some(body as usize), next],
        Op::Ret(_) | Op::RetNone => [None, None],
        _ => [next, None],
    }
}

/// Per instruction: are the views made at entry surely still valid there, that
/// is, no helper that may move a container ran since compiled code was entered?
/// `bumps[ip]`: instruction ip calls such a helper.
fn fresh_at(code: &[Op], runs: &[bool], bumps: &[bool]) -> Vec<bool> {
    let n = code.len();
    // optimistic start, then lowered: a successor of a stale point is stale
    let mut stale = vec![false; n];
    let mut changed = true;
    while changed {
        changed = false;
        for ip in 0..n {
            if !runs[ip] {
                continue; // compiled code leaves there; it comes back through an entry
            }
            let out_stale = stale[ip] || bumps[ip];
            if out_stale {
                for t in successors(code, ip).into_iter().flatten() {
                    if !stale[t] {
                        stale[t] = true;
                        changed = true;
                    }
                }
            }
        }
    }
    stale.iter().map(|s| !s).collect()
}

/// The constants pass 0 found (as `consts` is filled right after).
fn consts_after(l: &[(bool, i64, bool)]) -> Vec<Option<i64>> {
    l.iter().map(|&(loaded, v, other)| if loaded && !other { Some(v) } else { None }).collect()
}

/// Helpers that change nothing but their destination register (and the output).
fn local_helper(name: &str) -> bool {
    matches!(
        name,
        "gw_load_str" | "gw_concat" | "gw_to_str" | "gw_cmp_str" | "gw_char_at" | "gw_print" | "gw_println" | "gw_new_list" | "gw_move" | "gw_get_global" | "gw_get_elem" | "gw_get_elem2" | "gw_length"
    )
}

fn written_reg(op: &Op) -> Option<u16> {
    Some(match *op {
        Op::LoadInt(d, _) | Op::LoadReal(d, _) | Op::LoadBool(d, _) | Op::Move(d, _) => d,
        Op::AddI(d, ..) | Op::SubI(d, ..) | Op::MulI(d, ..) | Op::DivI(d, ..) | Op::ModI(d, ..) => d,
        Op::AddR(d, ..) | Op::SubR(d, ..) | Op::MulR(d, ..) | Op::DivR(d, ..) => d,
        Op::NegI(d, _) | Op::NegR(d, _) | Op::Not(d, _) | Op::ToReal(d, _) | Op::Length(d, _) => d,
        Op::CmpI(_, d, ..) | Op::CmpR(_, d, ..) | Op::CmpB(_, d, ..) | Op::CmpIImm(_, d, ..) => d,
        Op::AddIImm(d, ..) | Op::SubIImm(d, ..) | Op::MulIImm(d, ..) | Op::DivIImm(d, ..) | Op::ModIImm(d, ..) => d,
        Op::ForStep { var, .. } => var,
        Op::Call { dst, .. } => dst,
        Op::GetElem(d, ..) | Op::GetElem2(d, ..) | Op::GetGlobal(d, _) => d,
        Op::LoadStr(d, _) | Op::Concat(d, ..) | Op::ToStr(d, _) | Op::CmpS(_, d, ..) | Op::CharAt(d, ..) | Op::NewList(d) => d,
        Op::Builtin { dst, .. } => dst,
        _ => return None,
    })
}

/// A container register's cached view: the address of its first element, its
/// length (rows for a matrix), its columns, and the epoch it was read in.
#[derive(Clone, Copy)]
struct ViewVars {
    ptr: Variable,
    len: Variable,
    /// columns of a matrix; the kind of an array or list
    cols: Variable,
    bits: Variable,
    ep: Variable,
}

/// A register as Cranelift variables.
#[derive(Clone, Copy)]
struct RegVars {
    i: Variable,
    r: Variable,
    b: Variable,
    t: Variable,
    /// what the register may hold: K_INT | K_REAL | K_BOOL | K_NONE
    m: u8,
    /// one kind of number only: its tag is a constant, not a variable
    ct: bool,
}

/// The register's tag: a constant for a register of one kind.
fn tag_of(b: &mut FunctionBuilder, v: RegVars) -> V {
    if v.ct {
        let t = match v.m & 7 {
            K_INT => TAG_INT,
            K_REAL => TAG_REAL,
            _ => TAG_BOOL,
        };
        return b.ins().iconst(types::I64, t);
    }
    b.use_var(v.t)
}

const K_INT: u8 = 1;
const K_REAL: u8 = 2;
const K_BOOL: u8 = 4;
const K_NONE: u8 = 8;
const K_ALL: u8 = 15;

/// No compact array seen in that register.
const SPEC_NONE: i8 = 99;

/// The kind bit of a tag.
fn kind_of_tag(t: i64) -> u8 {
    match t {
        TAG_INT => K_INT,
        TAG_REAL => K_REAL,
        TAG_BOOL => K_BOOL,
        _ => K_NONE,
    }
}

type V = cranelift_codegen::ir::Value;

impl Jit {
    /// `None` when this machine has no Cranelift backend.
    pub fn new(module: &Module, call_body: *const u8) -> Option<Jit> {
        let mut flags = settings::builder();
        flags.set("opt_level", "speed").ok()?;
        flags.set("use_colocated_libcalls", "false").ok()?;
        flags.set("is_pic", "false").ok()?;
        let isa = cranelift_native::builder().ok()?.finish(settings::Flags::new(flags)).ok()?;
        let mut jb = JITBuilder::with_isa(isa, default_libcall_names());
        for (name, _, _) in HELPERS {
            jb.symbol(name, if name == "gw_call_body" { call_body } else { helper_ptr(name) });
        }
        let mut jm = JITModule::new(jb);
        let ptr = jm.isa().pointer_type();
        let mut helpers = HashMap::new();
        for (name, n, out) in HELPERS {
            let mut sig = jm.make_signature();
            sig.params.push(AbiParam::new(ptr));
            for _ in 0..n {
                sig.params.push(AbiParam::new(types::I64));
            }
            if name == "gw_builtin" || name == "gw_call_body" || name == "gw_par" {
                sig.params.push(AbiParam::new(ptr)); // the arguments
            }
            if out {
                sig.params.push(AbiParam::new(ptr));
            }
            sig.returns.push(AbiParam::new(types::I64));
            helpers.insert(name, jm.declare_function(name, Linkage::Import, &sig).ok()?);
        }
        // eligible functions: every instruction handled, every call to an eligible
        // function; repeated until nothing changes (a call can make its caller fall out)
        let n = module.functions.len();
        let mut eligible: Vec<bool> = module
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| i != 0 && f.code.iter().all(|op| plain_numeric(op) || matches!(op, Op::Call { .. } | Op::Ret(_) | Op::RetNone)))
            .collect();
        loop {
            let mut changed = false;
            for i in 0..n {
                if eligible[i] {
                    let ok = module.functions[i].code.iter().all(|op| match op {
                        Op::Call { func, .. } => eligible.get(*func as usize).copied().unwrap_or(false),
                        _ => true,
                    });
                    if !ok {
                        eligible[i] = false;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Some(Jit {
            log: std::env::var_os("GLOBW_JIT_LOG").is_some(),
            ctx: jm.make_context(),
            module: jm,
            fctx: FunctionBuilderContext::new(),
            helpers,
            layout: if std::env::var_os("GLOBW_JIT_NO_INLINE").is_some() { None } else { probe_layout() },
            eligible,
            funcs: (0..n).map(|_| None).collect(),
            compiled_funcs: false,
            calls: vec![0; n],
            func_bails: vec![0; n],
            heat: vec![0; n],
            state: vec![0; n],
            bodies: Vec::new(),
            runs: vec![Vec::new(); n],
            enter: vec![Vec::new(); n],
            par: Vec::new(),
            stops: vec![Vec::new(); n],
            next_name: 0,
        })
    }

    fn typed_signature(&self, n_params: usize) -> Signature {
        let mut sig = self.module.make_signature();
        for _ in 0..n_params {
            sig.params.push(AbiParam::new(types::I64));
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.params.push(AbiParam::new(types::I64)); // the call budget
        sig.returns.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        sig
    }

    fn fresh_name(&mut self, what: &str) -> String {
        self.next_name += 1;
        format!("gw_{}_{}", what, self.next_name)
    }

    /// Compiles every eligible function (once), so they can call each other.
    fn compile_functions(&mut self, m: &Module) -> bool {
        if self.compiled_funcs {
            return true;
        }
        self.compiled_funcs = true;
        let mut ids: Vec<Option<FuncId>> = vec![None; m.functions.len()];
        for (i, f) in m.functions.iter().enumerate() {
            if self.eligible[i] {
                let sig = self.typed_signature(f.n_params as usize);
                let name = self.fresh_name("fn");
                ids[i] = self.module.declare_function(&name, Linkage::Local, &sig).ok();
                if ids[i].is_none() {
                    self.eligible[i] = false;
                }
            }
        }
        let mut tramps: Vec<Option<FuncId>> = vec![None; m.functions.len()];
        for (i, f) in m.functions.iter().enumerate() {
            let Some(id) = ids[i] else { continue };
            if let Err(e) = self.build_function(f, id, &ids) {
                if self.log {
                    eprintln!("[jit] functions not compiled: {}", e);
                }
                return false;
            }
            tramps[i] = self.build_trampoline(f, id).ok();
        }
        if self.module.finalize_definitions().is_err() {
            return false;
        }
        if self.log {
            let names: Vec<&str> = (0..m.functions.len()).filter(|&i| ids[i].is_some()).map(|i| m.functions[i].name.as_str()).collect();
            eprintln!("[jit] functions: {}", names.join(", "));
        }
        for i in 0..m.functions.len() {
            if let (Some(id), Some(t)) = (ids[i], tramps[i]) {
                let p = self.module.get_finalized_function(t);
                // SAFETY: `t` was built with the Trampoline signature
                self.funcs[i] = Some((id, unsafe { std::mem::transmute::<*const u8, Trampoline>(p) }));
            }
        }
        true
    }

    /// The compiled entry of function `f`, if it is eligible and hot.
    pub fn function(&mut self, m: &Module, f: usize) -> Option<Trampoline> {
        if !self.eligible.get(f).copied().unwrap_or(false) || self.func_bails[f] >= MAX_BAILS {
            return None;
        }
        if !self.compiled_funcs {
            self.calls[f] += 1;
            if self.calls[f] < HOT_CALLS || !self.compile_functions(m) {
                return None;
            }
        }
        self.funcs[f].map(|(_, t)| t)
    }

    /// Compiled code for function `f` gave up.
    pub fn function_bailed(&mut self, f: usize) {
        self.func_bails[f] += 1;
        if self.log {
            eprintln!("[jit] function {} gave up ({})", f, self.func_bails[f]);
        }
    }

    /// Function `f` was called (`weight` 25) or one of its loops turned (1): once
    /// hot enough, its whole body is compiled.
    pub fn heat(&mut self, m: &Module, f: usize, weight: u32, regs: &[Value]) {
        if self.state[f] != 0 {
            return;
        }
        self.heat[f] += weight;
        if self.heat[f] < HOT_EDGES {
            return;
        }
        let code = self.compile_body(m, f, regs);
        if self.log {
            eprintln!("[jit] {} ({}) {}", m.functions[f].name, f, if code.is_some() { "compiled" } else { "not compilable" });
        }
        match code {
            Some(c) => {
                self.bodies.push(c);
                self.state[f] = COMPILED + (self.bodies.len() - 1) as u32;
            }
            None => self.state[f] = FAILED,
        }
    }

    /// Is function `f`'s body compiled?
    pub fn compiled(&self, f: usize) -> bool {
        self.state[f] >= COMPILED && self.state[f] != FAILED
    }

    /// The compiled body of `f`, if it can start at instruction `ip`.
    pub fn entry(&self, f: usize, ip: usize) -> Option<&BodyCode> {
        let s = self.state[f];
        if s < COMPILED || s == FAILED || !self.runs[f].get(ip).copied().unwrap_or(false) || !self.enter[f][ip] {
            return None;
        }
        Some(&self.bodies[(s - COMPILED) as usize])
    }

    /// Can the compiled body of `f` run instruction `ip` (wherever it entered)?
    pub fn runs_at(&self, f: usize, ip: usize) -> bool {
        self.compiled(f) && self.runs[f].get(ip).copied().unwrap_or(false)
    }

    /// Compiled code stopped at instruction `ip` of `f`, one it can run (an error
    /// for the interpreter to report, a call that gave up): after too many stops
    /// there, that instruction is left to the interpreter.
    pub fn stopped(&mut self, f: usize, ip: usize) {
        if let Some(c) = self.stops[f].get_mut(ip) {
            *c += 1;
            if *c as u32 >= MAX_DEOPTS {
                self.runs[f][ip] = false;
                if self.log {
                    eprintln!("[jit] {}:{} left to the interpreter", f, ip);
                }
            }
        }
    }

    /// Can compiled code run this instruction (with the native functions known)?
    fn runs_op(&self, op: &Op) -> bool {
        match op {
            Op::Call { .. } => true,
            Op::Builtin { id, .. } => callable_builtin(*id),
            op => plain_numeric(op) || loop_extra(op),
        }
    }

    fn compile_body(&mut self, m: &Module, f: usize, regs: &[Value]) -> Option<BodyCode> {
        let func = &m.functions[f];
        // without a loop a body does too little per call to pay for entering
        // compiled code: the interpreter runs it faster
        let has_loop = func.code.iter().enumerate().any(|(ip, op)| {
            matches!(*op, Op::Jump(t) | Op::JumpIfFalse(_, t) | Op::JumpIfTrue(_, t) | Op::JumpUnlessI(_, _, _, t) | Op::JumpUnlessIImm(_, _, _, t) if (t as usize) <= ip)
                || matches!(op, Op::ForStep { .. })
        });
        if !has_loop {
            return None;
        }
        let gives: Vec<bool> = m.functions.iter().map(|g| g.code.iter().any(|op| matches!(op, Op::Ret(_)))).collect();
        // calls to numeric functions go straight to their machine code
        if func.code.iter().any(|op| matches!(op, Op::Call { func, .. } if self.eligible.get(*func as usize).copied().unwrap_or(false))) {
            self.compile_functions(m);
        }
        let runs: Vec<bool> = func.code.iter().map(|op| self.runs_op(op)).collect();
        if !runs.iter().any(|&r| r) {
            return None;
        }
        let enter = entries(&func.code, &runs);
        // the kind of each compact array the frame holds now: code is made for it
        let spec: Vec<i8> = (0..func.n_regs as usize)
            .map(|r| match regs.get(r) {
                Some(Value::Packed(p)) => match &*p.borrow() {
                    crate::vm::Packed::Ints(..) => 0,
                    crate::vm::Packed::Reals(..) => 1,
                    crate::vm::Packed::Bools(..) => 2,
                },
                _ => SPEC_NONE,
            })
            .collect();
        let mut used = Vec::new();
        let mut written = Vec::new();
        for (op, &r) in func.code.iter().zip(&runs) {
            if r {
                regs_of(op, &mut used);
                if let Some(w) = written_reg(op) {
                    written.push(w);
                }
            }
        }
        used.sort_unstable();
        used.dedup();
        written.sort_unstable();
        written.dedup();
        let ids: Vec<Option<FuncId>> = self.funcs.iter().map(|x| x.map(|(id, _)| id)).collect();
        let id = match self.build_body(func, &runs, &enter, &used, &written, &ids, &gives, &spec) {
            Ok(id) => id,
            Err(e) => {
                if self.log {
                    eprintln!("[jit] {}: {}", func.name, e);
                }
                return None;
            }
        };
        self.module.finalize_definitions().ok()?;
        let p = self.module.get_finalized_function(id);
        // SAFETY: built with the BodyFn signature
        let entry = unsafe { std::mem::transmute::<*const u8, BodyFn>(p) };
        self.stops[f] = vec![0; func.code.len()];
        self.runs[f] = runs;
        self.enter[f] = enter;
        Some(BodyCode { entry, used, written })
    }

    // ------------------------------------------------------------------ codegen

    fn build_function(&mut self, f: &Function, id: FuncId, ids: &[Option<FuncId>]) -> Result<(), String> {
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = self.typed_signature(f.n_params as usize);
        self.ctx.func.name = UserFuncName::user(0, id.as_u32());
        let n_regs = f.n_regs as usize;
        let code = &f.code;
        let mut callee_refs = HashMap::new();
        let fc = self.module.isa().frontend_config();
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            for (i, fid) in ids.iter().enumerate() {
                if let Some(fid) = fid {
                    if code.iter().any(|op| matches!(op, Op::Call { func, .. } if *func as usize == i)) {
                        callee_refs.insert(i, self.module.declare_func_in_func(*fid, b.func));
                    }
                }
            }
            let vars: Vec<RegVars> = (0..n_regs).map(|_| declare_reg(&mut b)).collect();
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let params = b.block_params(entry).to_vec();
            let budget = *params.last().unwrap();
            let bail = b.create_block();
            // too many nested compiled calls: let the interpreter go on
            let start = b.create_block();
            let low = b.ins().icmp_imm_s(IntCC::SignedLessThanOrEqual, budget, 0);
            b.ins().brif(low, bail, &[], start, &[]);
            b.switch_to_block(start);
            let zero = b.ins().iconst(types::I64, 0);
            let zf = b.ins().f64const(0.0);
            let zb = b.ins().iconst(types::I8, 0);
            for (r, v) in vars.iter().enumerate() {
                if r < f.n_params as usize {
                    set_from_bits(&mut b, *v, params[2 * r], params[2 * r + 1]);
                } else {
                    b.def_var(v.i, zero);
                    b.def_var(v.r, zf);
                    b.def_var(v.b, zb);
                    b.def_var(v.t, zero);
                }
            }
            let blocks: Vec<Block> = (0..code.len()).map(|_| b.create_block()).collect();
            b.ins().jump(blocks[0], &[]);
            let g = Gen {
                vars: &vars,
                blocks: &blocks,
                lo: 0,
                hi: code.len(),
                leave: None,
                exits: std::cell::RefCell::new(HashMap::new()),
                bail,
                budget,
                callee_refs: &callee_refs,
                helper_refs: &HashMap::new(),
                ctxv: None,
                out: None,
                views: &[],
                epoch: None,
                layout: None,
                gives: &[],
                seen: None,
                moves: None,
                lconst: None,
                loading: std::cell::Cell::new(false),
                consts: &[],
                spec: &[],
                cur: std::cell::Cell::new(0),
                move_ips: None,
                clob_ips: None,
                clob: &[],
                bumps: None,
                fresh: &[],
                par: &HashMap::new(),
                heads: &HashMap::new(),
                fastv: &HashMap::new(),
                in_fast: std::cell::Cell::new(None),
            };
            for (ip, op) in code.iter().enumerate() {
                b.switch_to_block(blocks[ip]);
                g.op(&mut b, ip, *op)?;
            }
            b.switch_to_block(bail);
            let z = b.ins().iconst(types::I64, 0);
            let t = b.ins().iconst(types::I64, TAG_BAIL);
            b.ins().return_(&[z, t]);
            b.seal_all_blocks();
            b.finalize(fc);
        }
        self.module.define_function(id, &mut self.ctx).map_err(|e| format!("{:?}", e))?;
        Ok(())
    }

    /// `fn(args: *const Slot, budget, out: *mut Slot)`: calls the typed function.
    fn build_trampoline(&mut self, f: &Function, typed: FuncId) -> Result<FuncId, String> {
        let ptr = self.module.isa().pointer_type();
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(ptr));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(ptr));
        let name = self.fresh_name("tramp");
        let id = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| format!("{:?}", e))?;
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = sig;
        self.ctx.func.name = UserFuncName::user(0, id.as_u32());
        let fc = self.module.isa().frontend_config();
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            let callee = self.module.declare_func_in_func(typed, b.func);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let p = b.block_params(entry).to_vec();
            let (args, budget, out) = (p[0], p[1], p[2]);
            let mut call_args = Vec::new();
            for k in 0..f.n_params as i32 {
                call_args.push(b.ins().load(types::I64, MemFlagsData::trusted(), args, k * 16));
                call_args.push(b.ins().load(types::I64, MemFlagsData::trusted(), args, k * 16 + 8));
            }
            call_args.push(budget);
            let call = b.ins().call(callee, &call_args);
            let res = b.inst_results(call).to_vec();
            b.ins().store(MemFlagsData::trusted(), res[0], out, 0);
            b.ins().store(MemFlagsData::trusted(), res[1], out, 8);
            b.ins().return_(&[]);
            b.seal_all_blocks();
            b.finalize(fc);
        }
        self.module.define_function(id, &mut self.ctx).map_err(|e| format!("{:?}", e))?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    /// Compiles the parallel loops of `f` (after the first pass, which gave the
    /// kinds, constants and fresh views).
    #[allow(clippy::too_many_arguments)]
    fn build_par_loops(&mut self, f: &Function, runs: &[bool], live: &[Vec<u64>], masks: &[u8], consts: &[Option<i64>], spec: &[i8], fresh: &[bool], clob: &[bool]) -> HashMap<usize, ParHook> {
        let mut hooks = HashMap::new();
        if self.layout.is_none() || std::env::var_os("GLOBW_JIT_NO_PAR").is_some() {
            return hooks;
        }
        if self.log {
            eprintln!("[jit] {}: {} loops may run in parallel", f.name, par_plans(&f.code, runs, live, masks, spec, fresh, clob).len());
        }
        for p in par_plans(&f.code, runs, live, masks, spec, fresh, clob) {
            match self.build_chunk(f, &p, masks, consts, spec) {
                Ok(chunk) => {
                    let info = Box::new(ParLoop {
                        chunk,
                        accs: p.accs.clone(),
                        writes: p.writes.clone(),
                        reads: p.reads.clone(),
                        align: p.align,
                        min_trip: if p.nested { 16 } else { 200_000 },
                    });
                    let addr = &*info as *const ParLoop as i64;
                    self.par.push(info);
                    if self.log {
                        eprintln!("[jit] {}: loop {}..{} runs in parallel (sums into {:?}, writes {:?})", f.name, p.fi, p.fs, p.accs, p.writes);
                    }
                    hooks.insert(p.fi, ParHook { info: addr, accs: p.accs, inputs: p.inputs, var: p.var, end: p.end, step: p.step, exit: p.exit, n_regs: f.n_regs as usize });
                }
                Err(e) => {
                    if self.log {
                        eprintln!("[jit] {}: loop {} not parallel: {}", f.name, p.fi, e);
                    }
                }
            }
        }
        hooks
    }

    fn build_chunk(&mut self, f: &Function, p: &ParPlan, masks: &[u8], consts: &[Option<i64>], spec: &[i8]) -> Result<ChunkFn, String> {
        let ptr = self.module.isa().pointer_type();
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(ptr));
        sig.params.push(AbiParam::new(ptr));
        sig.returns.push(AbiParam::new(types::I64));
        let name = self.fresh_name("par");
        let id = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| format!("{:?}", e))?;
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = sig;
        self.ctx.func.name = UserFuncName::user(0, id.as_u32());
        let code = &f.code[..];
        let n_regs = f.n_regs as usize;
        let fc = self.module.isa().frontend_config();
        let empty_calls = HashMap::new();
        let empty_helpers = HashMap::new();
        let no_par = HashMap::new();
        let all_fresh = vec![true; code.len()];
        let no_clob = vec![false; n_regs];
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            let vars: Vec<RegVars> = (0..n_regs)
                .map(|r| {
                    let mut v = declare_reg(&mut b);
                    v.m = masks[r];
                    v
                })
                .collect();
            let views: Vec<ViewVars> = (0..n_regs)
                .map(|_| ViewVars { ptr: b.declare_var(types::I64), len: b.declare_var(types::I64), cols: b.declare_var(types::I64), bits: b.declare_var(types::I64), ep: b.declare_var(types::I64) })
                .collect();
            let epoch = b.declare_var(types::I64);
            let mins: Vec<Variable> = p.accs.iter().map(|_| b.declare_var(types::I64)).collect();
            let maxs: Vec<Variable> = p.accs.iter().map(|_| b.declare_var(types::I64)).collect();
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let prm = b.block_params(entry).to_vec();
            let (lo, hi, buf, out) = (prm[0], prm[1], prm[2], prm[3]);
            let zero = b.ins().iconst(types::I64, 0);
            let zf = b.ins().f64const(0.0);
            let zb = b.ins().iconst(types::I8, 0);
            b.def_var(epoch, zero);
            for (r, v) in vars.iter().enumerate() {
                b.def_var(v.i, zero);
                b.def_var(v.r, zf);
                b.def_var(v.b, zb);
                b.def_var(v.t, zero);
                let vv = views[r];
                for x in [vv.ptr, vv.len, vv.cols, vv.bits, vv.ep] {
                    b.def_var(x, zero);
                }
            }
            for &r in &p.inputs {
                let base = (r as usize * PAR_SLOT * 8) as i32;
                let bits = b.ins().load(types::I64, MemFlagsData::trusted(), buf, base);
                let tag = b.ins().load(types::I64, MemFlagsData::trusted(), buf, base + 8);
                set_from_bits(&mut b, vars[r as usize], bits, tag);
                if let Some(&k) = spec.get(r as usize).filter(|&&k| k != SPEC_NONE) {
                    let vv = views[r as usize];
                    let pp = b.ins().load(types::I64, MemFlagsData::trusted(), buf, base + 16);
                    let n = b.ins().load(types::I64, MemFlagsData::trusted(), buf, base + 24);
                    let vb = b.ins().load(types::I64, MemFlagsData::trusted(), buf, base + 32);
                    let kc = b.ins().iconst(types::I64, k as i64);
                    b.def_var(vv.ptr, pp);
                    b.def_var(vv.len, n);
                    b.def_var(vv.bits, vb);
                    b.def_var(vv.cols, kc);
                }
            }
            // each piece sums from zero
            for (k, &a) in p.accs.iter().enumerate() {
                b.def_var(vars[a as usize].i, zero);
                let t = b.ins().iconst(types::I64, TAG_INT);
                b.def_var(vars[a as usize].t, t);
                b.def_var(mins[k], zero);
                b.def_var(maxs[k], zero);
            }
            b.def_var(vars[p.var as usize].i, lo);
            let bail = b.create_block();
            let done = b.create_block();
            let lo_ip = p.fi + 1;
            let blocks: Vec<Block> = (lo_ip..=p.fs).map(|_| b.create_block()).collect();
            let budget = b.ins().iconst(types::I64, 0);
            b.ins().jump(blocks[0], &[]);
            let g = Gen {
                vars: &vars,
                blocks: &blocks,
                lo: lo_ip,
                hi: p.fs + 1,
                leave: None,
                exits: std::cell::RefCell::new(HashMap::new()),
                bail,
                budget,
                callee_refs: &empty_calls,
                helper_refs: &empty_helpers,
                ctxv: None,
                out: None,
                views: &views,
                epoch: Some(epoch),
                layout: self.layout,
                gives: &[],
                seen: None,
                moves: None,
                lconst: None,
                loading: std::cell::Cell::new(false),
                consts,
                spec,
                cur: std::cell::Cell::new(0),
                move_ips: None,
                clob_ips: None,
                clob: &no_clob,
                bumps: None,
                fresh: &all_fresh,
                par: &no_par,
                heads: &HashMap::new(),
                fastv: &HashMap::new(),
                in_fast: std::cell::Cell::new(None),
            };
            for ip in lo_ip..p.fs {
                b.switch_to_block(blocks[ip - lo_ip]);
                g.cur.set(ip);
                let aff = p.affine.iter().find(|&&(i, _, _)| i == ip).copied();
                match (aff, code[ip]) {
                    (Some((_, k, _)), Op::Move(..)) => {
                        let _ = k;
                        g.op(&mut b, ip, code[ip])?;
                    }
                    (Some((_, k, der)), op) => {
                        let dv = g.i(&mut b, der);
                        let (d, nv, of) = match op {
                            Op::AddI(d, x, y) => {
                                let other = g.i(&mut b, if x == der { y } else { x });
                                let (v, o) = b.ins().sadd_overflow(dv, other);
                                (d, v, o)
                            }
                            Op::AddIImm(d, _, n) => {
                                let c = b.ins().iconst(types::I64, n);
                                let (v, o) = b.ins().sadd_overflow(dv, c);
                                (d, v, o)
                            }
                            Op::SubI(d, _, y) => {
                                let other = g.i(&mut b, y);
                                let (v, o) = b.ins().ssub_overflow(dv, other);
                                (d, v, o)
                            }
                            Op::SubIImm(d, _, n) => {
                                let c = b.ins().iconst(types::I64, n);
                                let (v, o) = b.ins().ssub_overflow(dv, c);
                                (d, v, o)
                            }
                            _ => return Err("internal: not affine".into()),
                        };
                        let cont = b.create_block();
                        b.ins().brif(of, bail, &[], cont, &[]);
                        b.switch_to_block(cont);
                        g.set_i(&mut b, d, nv);
                        let mn = b.use_var(mins[k]);
                        let mx = b.use_var(maxs[k]);
                        let mn2 = b.ins().smin(mn, nv);
                        let mx2 = b.ins().smax(mx, nv);
                        b.def_var(mins[k], mn2);
                        b.def_var(maxs[k], mx2);
                        let nb = g.block(&mut b, ip + 1);
                        b.ins().jump(nb, &[]);
                    }
                    (None, op) => g.op(&mut b, ip, op)?,
                }
            }
            // the loop's own step: next turn of this piece, or done
            b.switch_to_block(blocks[p.fs - lo_ip]);
            let v = b.use_var(vars[p.var as usize].i);
            let nv = b.ins().iadd_imm_s(v, 1);
            b.def_var(vars[p.var as usize].i, nv);
            let again = b.ins().icmp(IntCC::SignedLessThanOrEqual, nv, hi);
            b.ins().brif(again, blocks[0], &[], done, &[]);
            b.switch_to_block(done);
            for (k, &a) in p.accs.iter().enumerate() {
                let s = b.use_var(vars[a as usize].i);
                let mn = b.use_var(mins[k]);
                let mx = b.use_var(maxs[k]);
                b.ins().store(MemFlagsData::trusted(), s, out, (k * 24) as i32);
                b.ins().store(MemFlagsData::trusted(), mn, out, (k * 24 + 8) as i32);
                b.ins().store(MemFlagsData::trusted(), mx, out, (k * 24 + 16) as i32);
            }
            let z = b.ins().iconst(types::I64, 0);
            b.ins().return_(&[z]);
            b.switch_to_block(bail);
            let one = b.ins().iconst(types::I64, 1);
            b.ins().return_(&[one]);
            b.seal_all_blocks();
            b.finalize(fc);
        }
        self.module.define_function(id, &mut self.ctx).map_err(|e| format!("{:?}", e))?;
        self.module.finalize_definitions().map_err(|e| format!("{:?}", e))?;
        let fp = self.module.get_finalized_function(id);
        // SAFETY: built with the ChunkFn signature
        Ok(unsafe { std::mem::transmute::<*const u8, ChunkFn>(fp) })
    }

    fn build_body(&mut self, f: &Function, runs: &[bool], enter: &[bool], used: &[u16], written: &[u16], ids: &[Option<FuncId>], gives: &[bool], spec: &[i8]) -> Result<FuncId, String> {
        let (start, end) = (0, f.code.len());
        let ptr = self.module.isa().pointer_type();
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(ptr));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(ptr));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let name = self.fresh_name("body");
        let id = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| format!("{:?}", e))?;
        let code = &f.code[..];
        let fc = self.module.isa().frontend_config();
        let n_regs = f.n_regs as usize;
        // pass 0 records the kinds each register is given; pass 1 builds with them
        let seen = std::cell::RefCell::new(vec![0u8; n_regs]);
        // per register: written only by LoadInt of one value (Some), else None
        let lconst = std::cell::RefCell::new(vec![(false, 0i64, false); n_regs]); // (loaded, value, other write)
        let mut consts: Vec<Option<i64>> = vec![None; n_regs];
        let bumps = std::cell::RefCell::new(vec![false; code.len()]);
        let move_ips = std::cell::RefCell::new(Vec::new());
        let clob_ips = std::cell::RefCell::new(Vec::new());
        let mut clob = vec![false; n_regs];
        let mut par_hooks: HashMap<usize, ParHook> = HashMap::new();
        let mut fresh: Vec<bool> = Vec::new();
        let moves = std::cell::RefCell::new(Vec::new());
        let mut masks: Option<Vec<u8>> = None;
        let live = live_in(code, runs, n_regs);
        for pass in 0..2 {
        self.module.clear_context(&mut self.ctx);
        self.ctx.set_disasm(pass == 1 && std::env::var_os("GLOBW_JIT_ASM").is_some());
        self.ctx.func.signature = sig.clone();
        self.ctx.func.name = UserFuncName::user(0, id.as_u32());
        let mut callee_refs = HashMap::new();
        let mut helper_refs = HashMap::new();
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            for (i, fid) in ids.iter().enumerate() {
                if let Some(fid) = fid {
                    if code[start..end].iter().any(|op| matches!(op, Op::Call { func, .. } if *func as usize == i)) {
                        callee_refs.insert(i, self.module.declare_func_in_func(*fid, b.func));
                    }
                }
            }
            for (name, id) in &self.helpers {
                helper_refs.insert(*name, self.module.declare_func_in_func(*id, b.func));
            }
            let out_slot = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 32, 3));
            let vars: Vec<RegVars> = (0..n_regs)
                .map(|r| {
                    let mut v = declare_reg(&mut b);
                    if let Some(m) = &masks {
                        v.m = m[r];
                        v.ct = live.is_some() && v.m & K_NONE == 0 && (v.m & 7).count_ones() == 1;
                    }
                    v
                })
                .collect();
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let p = b.block_params(entry).to_vec();
            let (slots, budget, ctxv, from) = (p[0], p[1], p[2], p[3]);
            let zero = b.ins().iconst(types::I64, 0);
            let mut kinds_ok = b.ins().iconst(types::I64, 1);
            let zf = b.ins().f64const(0.0);
            let zb = b.ins().iconst(types::I8, 0);
            for (r, v) in vars.iter().enumerate() {
                if used.binary_search(&(r as u16)).is_ok() {
                    let bits = b.ins().load(types::I64, MemFlagsData::trusted(), slots, (r * 16) as i32);
                    let tag = b.ins().load(types::I64, MemFlagsData::trusted(), slots, (r * 16 + 8) as i32);
                    set_from_bits(&mut b, *v, bits, tag);
                    if let Some(c) = consts.get(r).copied().flatten() {
                        let same = b.ins().icmp_imm_s(IntCC::Equal, bits, c);
                        let is_int = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_INT);
                        let both = b.ins().band(same, is_int);
                        let x = b.ins().uextend(types::I64, both);
                        kinds_ok = b.ins().band(kinds_ok, x);
                    }
                    if v.m != K_ALL {
                        // tags 0 (not a number), 1 int, 2 real, 3 bool, 4 no value yet:
                        // bit `tag` of the allowed set (a constant tag: its kind, or no value)
                        let set = if v.ct { ((v.m & 7) << 1) | 16 } else { 1 | ((v.m & 7) << 1) | 16 };
                        let allowed = b.ins().iconst(types::I64, set as i64);
                        let sh = b.ins().ushr(allowed, tag);
                        let bit = b.ins().band_imm_s(sh, 1);
                        let small = b.ins().icmp_imm_s(IntCC::UnsignedLessThan, tag, 5);
                        let okk = b.ins().uextend(types::I64, small);
                        let this = b.ins().band(bit, okk);
                        kinds_ok = b.ins().band(kinds_ok, this);
                    }
                } else {
                    b.def_var(v.i, zero);
                    b.def_var(v.r, zf);
                    b.def_var(v.b, zb);
                    b.def_var(v.t, zero);
                }
            }
            let out = b.ins().stack_addr(ptr, out_slot, 0);
            // views of containers: all stale at the start (epoch 0, views -1)
            let views: Vec<ViewVars> = (0..n_regs)
                .map(|_| ViewVars { ptr: b.declare_var(types::I64), len: b.declare_var(types::I64), cols: b.declare_var(types::I64), bits: b.declare_var(types::I64), ep: b.declare_var(types::I64) })
                .collect();
            let epoch = b.declare_var(types::I64);
            b.def_var(epoch, zero);
            let minus1 = b.ins().iconst(types::I64, -1);
            for v in &views {
                b.def_var(v.ptr, zero);
                b.def_var(v.len, zero);
                b.def_var(v.cols, zero);
                b.def_var(v.bits, zero);
                b.def_var(v.ep, minus1);
            }
            if pass == 1 && fresh.iter().any(|&f| f) {
                let fref = *helper_refs.get("gw_seq_view").ok_or("no view helper")?;
                for (r, &k) in spec.iter().enumerate() {
                    if k == SPEC_NONE || clob[r] || used.binary_search(&(r as u16)).is_err() {
                        continue;
                    }
                    let rc = b.ins().iconst(types::I64, r as i64);
                    let call = b.ins().call(fref, &[ctxv, rc, out]);
                    let st = b.inst_results(call)[0];
                    let p = b.ins().load(types::I64, MemFlagsData::trusted(), out, 0);
                    let n = b.ins().load(types::I64, MemFlagsData::trusted(), out, 8);
                    let kind = b.ins().load(types::I64, MemFlagsData::trusted(), out, 16);
                    let bits = b.ins().load(types::I64, MemFlagsData::trusted(), out, 24);
                    let vv = views[r];
                    b.def_var(vv.ptr, p);
                    b.def_var(vv.len, n);
                    b.def_var(vv.cols, kind);
                    b.def_var(vv.bits, bits);
                    b.def_var(vv.ep, zero);
                    // a container of the kind seen, or no fast code from here
                    let okv = b.ins().icmp_imm_s(IntCC::Equal, st, 0);
                    let okk = b.ins().icmp_imm_s(IntCC::Equal, kind, k as i64);
                    let both = b.ins().band(okv, okk);
                    let x = b.ins().uextend(types::I64, both);
                    kinds_ok = b.ins().band(kinds_ok, x);
                }
            }
            let blocks: Vec<Block> = (start..end).map(|_| b.create_block()).collect();
            // the one way out: write the registers back, return where to go on
            let leave = b.create_block();
            b.append_block_param(leave, types::I64);
            let bail = b.create_block(); // an unknown entry: back to the interpreter
            let mut heads: HashMap<usize, (Block, Variable)> = HashMap::new();
            let mut fastv: HashMap<usize, FastLoop> = HashMap::new();
            if pass == 1 && self.layout.is_some() && std::env::var_os("GLOBW_JIT_NO_VERSION").is_none() {
                for (fi, fs, var, end, step, conts) in fast_loops(code, spec, &fresh, &clob) {
                    if !runs[fi + 1..=fs].iter().all(|&r| r) {
                        continue;
                    }
                    let blocks: Vec<Block> = (fi + 1..=fs).map(|_| b.create_block()).collect();
                    fastv.insert(fi + 1, FastLoop { fs, var, end, step, conts, choose: b.create_block(), blocks });
                }
            }
            for &fi in par_hooks.keys() {
                let flag = b.declare_var(types::I8);
                let z = b.ins().iconst(types::I8, 0);
                b.def_var(flag, z);
                heads.insert(fi + 1, (b.create_block(), flag));
            }
            // enter at instruction `from`
            let calls: Vec<_> = (start..end)
                .map(|ip| {
                    let blk = if !(runs[ip] && enter[ip]) {
                        bail
                    } else if let Some(&(pe, _)) = heads.get(&ip) {
                        pe
                    } else if let Some(fl) = fastv.get(&ip) {
                        fl.choose
                    } else {
                        blocks[ip - start]
                    };
                    b.func.dfg.block_call(blk, &[])
                })
                .collect();
            let default = b.func.dfg.block_call(bail, &[]);
            let jt = b.create_jump_table(cranelift_codegen::ir::JumpTableData::new(default, &calls));
            // a register holding a kind this code does not expect: the interpreter goes on
            let dispatch = b.create_block();
            b.ins().brif(kinds_ok, dispatch, &[], bail, &[]);
            b.switch_to_block(dispatch);
            let from32 = b.ins().ireduce(types::I32, from);
            b.ins().br_table(from32, jt);
            let g = Gen {
                vars: &vars,
                blocks: &blocks,
                lo: start,
                hi: end,
                leave: Some(leave),
                exits: std::cell::RefCell::new(HashMap::new()),
                bail,
                budget,
                callee_refs: &callee_refs,
                helper_refs: &helper_refs,
                ctxv: Some(ctxv),
                out: Some(out),
                views: &views,
                epoch: Some(epoch),
                layout: self.layout,
                gives,
                seen: if pass == 0 { Some(&seen) } else { None },
                moves: if pass == 0 { Some(&moves) } else { None },
                lconst: if pass == 0 { Some(&lconst) } else { None },
                loading: std::cell::Cell::new(false),
                consts: &consts,
                spec,
                cur: std::cell::Cell::new(0),
                move_ips: if pass == 0 { Some(&move_ips) } else { None },
                clob_ips: if pass == 0 { Some(&clob_ips) } else { None },
                clob: &clob,
                bumps: if pass == 0 { Some(&bumps) } else { None },
                fresh: &fresh,
                par: &par_hooks,
                heads: &heads,
                fastv: &fastv,
                in_fast: std::cell::Cell::new(None),
            };
            for (&head, &(pe, flag)) in heads.iter() {
                b.switch_to_block(pe);
                let tried = b.use_var(flag);
                let attempt = b.create_block();
                b.ins().brif(tried, blocks[head - start], &[], attempt, &[]);
                b.switch_to_block(attempt);
                let one = b.ins().iconst(types::I8, 1);
                b.def_var(flag, one);
                g.par_hook(&mut b, &par_hooks[&(head - 1)])?;
                let to = fastv.get(&head).map_or(blocks[head - start], |fl| fl.choose);
                b.ins().jump(to, &[]);
            }
            // choosers: the check-free copy when every turn left stays in bounds
            for (&head, fl) in fastv.iter() {
                b.switch_to_block(fl.choose);
                let v = g.i(&mut b, fl.var);
                let e = g.i(&mut b, fl.end);
                let st = g.i(&mut b, fl.step);
                let s1 = b.ins().icmp_imm_s(IntCC::Equal, st, 1);
                let v1 = b.ins().icmp_imm_s(IntCC::SignedGreaterThanOrEqual, v, 1);
                let mut ok = b.ins().band(s1, v1);
                for &c in &fl.conts {
                    if matches!(spec.get(c as usize), Some(0) | Some(1)) {
                        let cc = b.ins().iconst(types::I64, c as i64);
                        let nb = g.helper(&mut b, "gw_try_full", &[cc])?;
                        b.def_var(views[c as usize].bits, nb);
                    }
                    let n = b.use_var(views[c as usize].len);
                    let le = b.ins().icmp(IntCC::SignedLessThanOrEqual, e, n);
                    ok = b.ins().band(ok, le);
                }
                b.ins().brif(ok, fl.blocks[0], &[], blocks[head - start], &[]);
            }
            // the check-free copies
            for (&head, fl) in fastv.iter() {
                g.in_fast.set(Some(head));
                for ip in head..=fl.fs {
                    b.switch_to_block(fl.blocks[ip - head]);
                    g.cur.set(ip);
                    g.op(&mut b, ip, code[ip])?;
                }
                g.in_fast.set(None);
            }
            for ip in start..end {
                b.switch_to_block(blocks[ip - start]);
                g.cur.set(ip);
                if runs[ip] {
                    g.op(&mut b, ip, code[ip])?;
                } else {
                    // the interpreter runs this one
                    let out_to = g.exit_to(&mut b, ip);
                    b.ins().jump(out_to, &[]);
                }
            }
            // exit blocks: each writes back the registers the interpreter reads from
            // there on (all written ones when that is not known) and returns
            let exits: Vec<(usize, Block)> = g.exits.borrow().iter().map(|(k, v)| (*k, *v)).collect();
            for (ip, blk) in exits {
                b.switch_to_block(blk);
                let c = b.ins().iconst(types::I64, ip as i64);
                match live.as_ref().and_then(|l| l.get(ip)) {
                    Some(l) => {
                        for &r in written {
                            if l[r as usize / 64] >> (r % 64) & 1 == 1 {
                                let v = vars[r as usize];
                                let bits = bits_of(&mut b, v);
                                let tag = tag_of(&mut b, v);
                                b.ins().store(MemFlagsData::trusted(), bits, slots, (r as usize * 16) as i32);
                                b.ins().store(MemFlagsData::trusted(), tag, slots, (r as usize * 16 + 8) as i32);
                            }
                        }
                        b.ins().return_(&[c]);
                    }
                    None => {
                        b.ins().jump(leave, &[c.into()]);
                    }
                }
            }
            b.switch_to_block(leave);
            let resume = b.block_params(leave)[0];
            for &r in written {
                let v = vars[r as usize];
                let bits = bits_of(&mut b, v);
                let tag = tag_of(&mut b, v);
                b.ins().store(MemFlagsData::trusted(), bits, slots, (r as usize * 16) as i32);
                b.ins().store(MemFlagsData::trusted(), tag, slots, (r as usize * 16 + 8) as i32);
            }
            b.ins().return_(&[resume]);
            b.switch_to_block(bail);
            b.ins().return_(&[from]);
            b.seal_all_blocks();
            b.finalize(fc);
        }
        if pass == 0 {
            // what moves carry, until nothing changes; never written here: anything
            // never written here: anything (also what it is moved into)
            let mut m: Vec<u8> = seen.borrow().iter().map(|&x| if x == 0 { K_ALL } else { x }).collect();
            loop {
                let mut changed = false;
                for &(d, s2) in moves.borrow().iter() {
                    let x = m[d as usize] | m[s2 as usize];
                    if x != m[d as usize] {
                        m[d as usize] = x;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            for x in m.iter_mut() {
                if *x == 0 {
                    *x = K_ALL;
                }
            }
            masks = Some(m);
            for &(ip, src) in move_ips.borrow().iter() {
                if masks.as_ref().is_some_and(|m| m[src as usize] & K_NONE != 0) {
                    if let Some(d) = written_reg(&code[ip]) {
                        clob[d as usize] = true;
                    }
                }
            }
            for &ip in clob_ips.borrow().iter() {
                if let Some(d) = written_reg(&code[ip]) {
                    clob[d as usize] = true;
                }
            }
            fresh = fresh_at(code, runs, &bumps.borrow());
            if let (Some(l), Some(m)) = (live.as_ref(), masks.as_ref()) {
                par_hooks = self.build_par_loops(f, runs, l, m, &consts_after(&lconst.borrow()), spec, &fresh, &clob);
            }
            if self.log {
                let b2 = bumps.borrow();
                eprintln!("[jit] bumps at {:?}; stale at {:?}", (0..code.len()).filter(|&i| b2[i]).collect::<Vec<_>>(), (0..code.len()).filter(|&i| !fresh[i]).collect::<Vec<_>>());
            }
            for (r, &(loaded, v, other)) in lconst.borrow().iter().enumerate() {
                if loaded && !other {
                    consts[r] = Some(v);
                }
            }
        }
        }
        self.module.define_function(id, &mut self.ctx).map_err(|e| format!("{:?}", e))?;
        if std::env::var_os("GLOBW_JIT_ASM").is_some() {
            if let Some(c) = self.ctx.compiled_code() {
                if let Some(v) = &c.vcode {
                    eprintln!("[jit asm] {}\n{}", f.name, v);
                }
            }
        }
        Ok(id)
    }
}

fn declare_reg(b: &mut FunctionBuilder) -> RegVars {
    RegVars { i: b.declare_var(types::I64), r: b.declare_var(types::F64), b: b.declare_var(types::I8), t: b.declare_var(types::I64), m: K_ALL, ct: false }
}

/// Sets a register's variables (those of the kinds it may hold) from raw bits and a tag.
fn set_from_bits(b: &mut FunctionBuilder, v: RegVars, bits: V, tag: V) {
    if v.m & K_INT != 0 {
        b.def_var(v.i, bits);
    }
    if v.m & K_REAL != 0 {
        let r = b.ins().bitcast(types::F64, MemFlagsData::new(), bits);
        b.def_var(v.r, r);
    }
    if v.m & K_BOOL != 0 {
        let bo = b.ins().ireduce(types::I8, bits);
        b.def_var(v.b, bo);
    }
    b.def_var(v.t, tag);
}

/// The register's current value as raw bits, chosen by its tag.
fn bits_of(b: &mut FunctionBuilder, v: RegVars) -> V {
    // a register of one kind: no choice to make
    match v.m & 7 {
        0 => return b.ins().iconst(types::I64, 0),
        K_INT => return b.use_var(v.i),
        K_REAL => {
            let r = b.use_var(v.r);
            return b.ins().bitcast(types::I64, MemFlagsData::new(), r);
        }
        K_BOOL => {
            let bo = b.use_var(v.b);
            return b.ins().uextend(types::I64, bo);
        }
        _ => {}
    }
    let tag = tag_of(b, v);
    let i = b.use_var(v.i);
    let r = b.use_var(v.r);
    let rb = b.ins().bitcast(types::I64, MemFlagsData::new(), r);
    let bo = b.use_var(v.b);
    let bb = b.ins().uextend(types::I64, bo);
    let is_real = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_REAL);
    let is_bool = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_BOOL);
    let x = b.ins().select(is_bool, bb, i);
    b.ins().select(is_real, rb, x)
}

fn int_cc(c: Cmp) -> IntCC {
    match c {
        Cmp::Eq => IntCC::Equal,
        Cmp::NotEq => IntCC::NotEqual,
        Cmp::Lt => IntCC::SignedLessThan,
        Cmp::LtEq => IntCC::SignedLessThanOrEqual,
        Cmp::Gt => IntCC::SignedGreaterThan,
        Cmp::GtEq => IntCC::SignedGreaterThanOrEqual,
    }
}

/// false < true, as the interpreter compares bools
fn bool_cc(c: Cmp) -> IntCC {
    match c {
        Cmp::Eq => IntCC::Equal,
        Cmp::NotEq => IntCC::NotEqual,
        Cmp::Lt => IntCC::UnsignedLessThan,
        Cmp::LtEq => IntCC::UnsignedLessThanOrEqual,
        Cmp::Gt => IntCC::UnsignedGreaterThan,
        Cmp::GtEq => IntCC::UnsignedGreaterThanOrEqual,
    }
}

/// Rust's comparisons on f64 (false whenever a NaN is involved, except `!=`).
fn float_cc(c: Cmp) -> FloatCC {
    match c {
        Cmp::Eq => FloatCC::Equal,
        Cmp::NotEq => FloatCC::NotEqual,
        Cmp::Lt => FloatCC::LessThan,
        Cmp::LtEq => FloatCC::LessThanOrEqual,
        Cmp::Gt => FloatCC::GreaterThan,
        Cmp::GtEq => FloatCC::GreaterThanOrEqual,
    }
}

/// Translates the instructions of one unit (a function or a loop).
struct Gen<'a> {
    vars: &'a [RegVars],
    blocks: &'a [Block],
    /// the unit covers instructions lo..hi
    lo: usize,
    hi: usize,
    /// a loop: the block that writes the registers back and returns
    leave: Option<Block>,
    /// a loop: one block per instruction it leaves to, made on demand
    exits: std::cell::RefCell<HashMap<usize, Block>>,
    /// a function: the block that gives up
    bail: Block,
    budget: V,
    callee_refs: &'a HashMap<usize, cranelift_codegen::ir::FuncRef>,
    helper_refs: &'a HashMap<&'static str, cranelift_codegen::ir::FuncRef>,
    ctxv: Option<V>,
    out: Option<V>,
    views: &'a [ViewVars],
    /// bumped by every helper that may move or replace a container
    epoch: Option<Variable>,
    layout: Option<Layout>,
    /// per function: does it give back a value
    gives: &'a [bool],
    /// the first pass: per register, the kinds written; and the moves
    seen: Option<&'a std::cell::RefCell<Vec<u8>>>,
    moves: Option<&'a std::cell::RefCell<Vec<(u16, u16)>>>,
    /// the first pass: LoadInt constants per register
    lconst: Option<&'a std::cell::RefCell<Vec<(bool, i64, bool)>>>,
    loading: std::cell::Cell<bool>,
    /// registers that always hold one int (the second pass)
    consts: &'a [Option<i64>],
    /// per register: the kind of compact array seen there (SPEC_NONE: none)
    spec: &'a [i8],
    /// the instruction being compiled
    cur: std::cell::Cell<usize>,
    /// the first pass: every move's instruction and source
    move_ips: Option<&'a std::cell::RefCell<Vec<(usize, u16)>>>,
    /// the first pass: instructions whose helper writes their destination only
    clob_ips: Option<&'a std::cell::RefCell<Vec<usize>>>,
    /// per register: may a helper replace what it holds (its entry view untrusted)
    clob: &'a [bool],
    /// the first pass: instructions that call a helper that may move containers
    bumps: Option<&'a std::cell::RefCell<Vec<bool>>>,
    /// the second pass: instructions where the entry views are surely valid
    fresh: &'a [bool],
    /// for loops that may run in parallel, by their ForInit
    par: &'a HashMap<usize, ParHook>,
    /// their heads (first body instruction): the block that tries the parallel
    /// run once, and the "tried" flag
    heads: &'a HashMap<usize, (Block, Variable)>,
    /// loops with a check-free copy, by head
    fastv: &'a HashMap<usize, FastLoop>,
    /// generating that copy (its head)
    in_fast: std::cell::Cell<Option<usize>>,
}

impl Gen<'_> {
    fn saw(&self, r: u16, k: u8) {
        if let Some(s) = self.seen {
            s.borrow_mut()[r as usize] |= k;
        }
        if let Some(l) = self.lconst {
            if !self.loading.get() {
                l.borrow_mut()[r as usize].2 = true;
            }
        }
    }
    /// The block for instruction `ip`: inside the unit, or (a loop) a way out to it.
    fn block(&self, b: &mut FunctionBuilder, ip: usize) -> Block {
        if let Some(h) = self.in_fast.get() {
            let fl = &self.fastv[&h];
            if ip >= h && ip <= fl.fs {
                return fl.blocks[ip - h];
            }
        }
        if let Some(&(pe, _)) = self.heads.get(&ip) {
            return pe;
        }
        if let Some(fl) = self.fastv.get(&ip) {
            return fl.choose;
        }
        if ip >= self.lo && ip < self.hi {
            return self.blocks[ip - self.lo];
        }
        self.exit_to(b, ip)
    }

    fn exit_to(&self, b: &mut FunctionBuilder, ip: usize) -> Block {
        if self.leave.is_none() {
            return self.bail;
        }
        *self.exits.borrow_mut().entry(ip).or_insert_with(|| b.create_block())
    }

    /// Where to go when instruction `ip` cannot run here: a function gives up, a
    /// loop leaves to that very instruction (the interpreter runs it).
    fn stop(&self, b: &mut FunctionBuilder, ip: usize) -> Block {
        self.exit_to(b, ip)
    }

    fn next(&self, b: &mut FunctionBuilder, ip: usize) -> Block {
        self.block(b, ip + 1)
    }

    fn i(&self, b: &mut FunctionBuilder, r: u16) -> V {
        if let Some(c) = self.consts.get(r as usize).copied().flatten() {
            return b.ins().iconst(types::I64, c);
        }
        b.use_var(self.vars[r as usize].i)
    }
    fn r(&self, b: &mut FunctionBuilder, r: u16) -> V {
        b.use_var(self.vars[r as usize].r)
    }
    fn bo(&self, b: &mut FunctionBuilder, r: u16) -> V {
        b.use_var(self.vars[r as usize].b)
    }
    fn tag(&self, b: &mut FunctionBuilder, r: u16, t: i64) {
        self.saw(r, kind_of_tag(t));
        let c = b.ins().iconst(types::I64, t);
        b.def_var(self.vars[r as usize].t, c);
    }
    fn set_i(&self, b: &mut FunctionBuilder, r: u16, v: V) {
        b.def_var(self.vars[r as usize].i, v);
        self.tag(b, r, TAG_INT);
    }
    fn set_r(&self, b: &mut FunctionBuilder, r: u16, v: V) {
        b.def_var(self.vars[r as usize].r, v);
        self.tag(b, r, TAG_REAL);
    }
    fn set_b(&self, b: &mut FunctionBuilder, r: u16, v: V) {
        b.def_var(self.vars[r as usize].b, v);
        self.tag(b, r, TAG_BOOL);
    }

    /// Goes on in a fresh block when `bad` is false; stops at `ip` when it is true.
    /// The destination is written only on the way on, so a stop leaves it as it was.
    fn guard(&self, b: &mut FunctionBuilder, ip: usize, bad: V) {
        let ok = b.create_block();
        let stop = self.stop(b, ip);
        b.ins().brif(bad, stop, &[], ok, &[]);
        b.switch_to_block(ok);
    }

    fn jump_next(&self, b: &mut FunctionBuilder, ip: usize) {
        let n = self.next(b, ip);
        b.ins().jump(n, &[]);
    }

    /// dst = checked add / sub / mul
    fn checked(&self, b: &mut FunctionBuilder, ip: usize, d: u16, x: V, y: V, which: u8) {
        let (v, of) = match which {
            0 => b.ins().sadd_overflow(x, y),
            1 => b.ins().ssub_overflow(x, y),
            _ => b.ins().smul_overflow(x, y),
        };
        self.guard(b, ip, of);
        self.set_i(b, d, v);
        self.jump_next(b, ip);
    }

    /// dst = x / y or x mod y; y == 0 and MIN / -1 are left to the interpreter
    fn divide(&self, b: &mut FunctionBuilder, ip: usize, d: u16, x: V, y: V, rem: bool) {
        let y0 = b.ins().icmp_imm_s(IntCC::Equal, y, 0);
        let xm = b.ins().icmp_imm_s(IntCC::Equal, x, i64::MIN);
        let ym = b.ins().icmp_imm_s(IntCC::Equal, y, -1);
        let both = b.ins().band(xm, ym);
        let bad = b.ins().bor(y0, both);
        self.guard(b, ip, bad);
        let v = if rem { b.ins().srem(x, y) } else { b.ins().sdiv(x, y) };
        self.set_i(b, d, v);
        self.jump_next(b, ip);
    }

    fn helper(&self, b: &mut FunctionBuilder, name: &str, args: &[V]) -> Result<V, String> {
        let f = *self.helper_refs.get(name).ok_or_else(|| format!("not compiled here: {}", name))?;
        let mut all = vec![self.ctxv.ok_or("no context")?];
        all.extend_from_slice(args);
        let call = b.ins().call(f, &all);
        let res = b.inst_results(call)[0];
        if !name.ends_with("_view") && name != "gw_par" && name != "gw_try_full" {
            // a helper that writes only its destination makes only that register's
            // view untrusted (a move: only if its source may hold a non-number,
            // decided after pass 0); any other may move any container
            if let Some(bm) = self.bumps {
                if local_helper(name) {
                    if name != "gw_move" {
                        if let Some(cl) = self.clob_ips {
                            cl.borrow_mut().push(self.cur.get());
                        }
                    }
                } else if let Some(x) = bm.borrow_mut().get_mut(self.cur.get()) {
                    *x = true;
                }
            }
            if let Some(e) = self.epoch {
                let x = b.use_var(e);
                let x1 = b.ins().iadd_imm_s(x, 1);
                b.def_var(e, x1);
            }
        }
        Ok(res)
    }

    /// Before a loop that may run in parallel: its registers into a buffer, then
    /// the helper; done there, the accumulators and counter take their final
    /// values and control goes past the loop. Otherwise the loop runs here.
    fn par_hook(&self, b: &mut FunctionBuilder, h: &ParHook) -> Result<(), String> {
        let ptr = types::I64;
        let size = (h.n_regs * PAR_SLOT * 8).max(8) as u32;
        let ss = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, size, 3));
        let buf = b.ins().stack_addr(ptr, ss, 0);
        for &r in &h.inputs {
            let v = self.vars[r as usize];
            let base = (r as usize * PAR_SLOT * 8) as i32;
            let bits = if let Some(c) = self.consts.get(r as usize).copied().flatten() { b.ins().iconst(types::I64, c) } else { bits_of(b, v) };
            let tag = tag_of(b, v);
            b.ins().store(MemFlagsData::trusted(), bits, buf, base);
            b.ins().store(MemFlagsData::trusted(), tag, buf, base + 8);
            if self.spec.get(r as usize).is_some_and(|&k| k != SPEC_NONE) {
                let vv = self.views[r as usize];
                let (p, n, vb) = (b.use_var(vv.ptr), b.use_var(vv.len), b.use_var(vv.bits));
                b.ins().store(MemFlagsData::trusted(), p, buf, base + 16);
                b.ins().store(MemFlagsData::trusted(), n, buf, base + 24);
                b.ins().store(MemFlagsData::trusted(), vb, buf, base + 32);
            }
        }
        let os = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, (h.accs.len().max(1) * 8) as u32, 3));
        let out = b.ins().stack_addr(ptr, os, 0);
        let (lo, hi, st) = (self.i(b, h.var), self.i(b, h.end), self.i(b, h.step));
        let (attempt, took, normal) = (b.create_block(), b.create_block(), b.create_block());
        let one = b.ins().icmp_imm_s(IntCC::Equal, st, 1);
        b.ins().brif(one, attempt, &[], normal, &[]);
        b.switch_to_block(attempt);
        let info = b.ins().iconst(types::I64, h.info);
        let r = self.helper(b, "gw_par", &[info, lo, hi, buf, out])?;
        let ok = b.ins().icmp_imm_s(IntCC::Equal, r, 0);
        b.ins().brif(ok, took, &[], normal, &[]);
        b.switch_to_block(took);
        for (k, &a) in h.accs.iter().enumerate() {
            let x = b.ins().load(types::I64, MemFlagsData::trusted(), out, (k * 8) as i32);
            self.set_i(b, a, x);
        }
        let past = b.ins().iadd_imm_s(hi, 1);
        self.set_i(b, h.var, past);
        let eb = self.block(b, h.exit);
        b.ins().jump(eb, &[]);
        b.switch_to_block(normal);
        Ok(())
    }

    fn konst(&self, b: &mut FunctionBuilder, n: i64) -> V {
        b.ins().iconst(types::I64, n)
    }

    /// After a helper that filled the `out` slot for register `d`: a number goes
    /// into its variables; anything else is already in the interpreter's register.
    fn take_out(&self, b: &mut FunctionBuilder, d: u16) {
        let out = self.out.expect("a loop");
        let bits = b.ins().load(types::I64, MemFlagsData::trusted(), out, 0);
        let tag = b.ins().load(types::I64, MemFlagsData::trusted(), out, 8);
        self.saw(d, K_ALL);
        set_from_bits(b, self.vars[d as usize], bits, tag);
    }

    /// The cached view of container `c` (refreshed when stale); goes to `slow`
    /// when the register holds no container of that kind.
    fn view(&self, b: &mut FunctionBuilder, c: u16, matrix: bool, slow: Block) -> Result<(V, V, V), String> {
        let vv = self.views[c as usize];
        let e = b.use_var(self.epoch.ok_or("no epoch")?);
        let ep = b.use_var(vv.ep);
        let stale = b.ins().icmp(IntCC::NotEqual, ep, e);
        let (refresh, fresh, have) = (b.create_block(), b.create_block(), b.create_block());
        b.ins().brif(stale, refresh, &[], have, &[]);
        b.switch_to_block(refresh);
        let cc = self.konst(b, c as i64);
        let out = self.out.ok_or("no out slot")?;
        let st = self.helper(b, if matrix { "gw_mat_view" } else { "gw_seq_view" }, &[cc, out])?;
        b.ins().brif(st, slow, &[], fresh, &[]);
        b.switch_to_block(fresh);
        let p = b.ins().load(types::I64, MemFlagsData::trusted(), out, 0);
        let n = b.ins().load(types::I64, MemFlagsData::trusted(), out, 8);
        b.def_var(vv.ptr, p);
        b.def_var(vv.len, n);
        // a matrix: columns; an array or list: its kind, and the bits
        let k = b.ins().load(types::I64, MemFlagsData::trusted(), out, 16);
        b.def_var(vv.cols, k);
        if !matrix {
            let bits = b.ins().load(types::I64, MemFlagsData::trusted(), out, 24);
            b.def_var(vv.bits, bits);
        }
        b.def_var(vv.ep, e);
        b.ins().jump(have, &[]);
        b.switch_to_block(have);
        Ok((b.use_var(vv.ptr), b.use_var(vv.len), b.use_var(vv.cols)))
    }

    /// The address of element `k` (0-based, already known in range).
    fn elem_addr(&self, b: &mut FunctionBuilder, ptr: V, k: V, lay: Layout) -> V {
        let off = b.ins().imul_imm_s(k, lay.size);
        b.ins().iadd(ptr, off)
    }

    /// Reads the element at `addr` into register `d` when it is a number or a bool.
    fn read_elem(&self, b: &mut FunctionBuilder, ip: usize, d: u16, addr: V, lay: Layout, slow: Block) {
        let t = b.ins().uload8(types::I64, MemFlagsData::trusted(), addr, 0);
        let (bi, nr, br, nb, bb) = (b.create_block(), b.create_block(), b.create_block(), b.create_block(), b.create_block());
        let is_i = b.ins().icmp_imm_s(IntCC::Equal, t, lay.t_int as i64);
        b.ins().brif(is_i, bi, &[], nr, &[]);
        b.switch_to_block(bi);
        let v = b.ins().load(types::I64, MemFlagsData::trusted(), addr, 8);
        self.set_i(b, d, v);
        self.jump_next(b, ip);
        b.switch_to_block(nr);
        let is_r = b.ins().icmp_imm_s(IntCC::Equal, t, lay.t_real as i64);
        b.ins().brif(is_r, br, &[], nb, &[]);
        b.switch_to_block(br);
        let v = b.ins().load(types::F64, MemFlagsData::trusted(), addr, 8);
        self.set_r(b, d, v);
        self.jump_next(b, ip);
        b.switch_to_block(nb);
        let is_b = b.ins().icmp_imm_s(IntCC::Equal, t, lay.t_bool as i64);
        b.ins().brif(is_b, bb, &[], slow, &[]);
        b.switch_to_block(bb);
        let v = b.ins().load(types::I8, MemFlagsData::trusted(), addr, 1);
        self.set_b(b, d, v);
        self.jump_next(b, ip);
    }

    /// Writes register `s` (a number or a bool) over the element at `addr`, when
    /// the element owns no memory (a number, a bool, or no value yet).
    fn write_elem(&self, b: &mut FunctionBuilder, ip: usize, s: u16, addr: V, lay: Layout, slow: Block) {
        let old = b.ins().uload8(types::I64, MemFlagsData::trusted(), addr, 0);
        let mut plain = None;
        for t in [lay.t_null, lay.t_int, lay.t_real, lay.t_bool] {
            let e = b.ins().icmp_imm_s(IntCC::Equal, old, t as i64);
            plain = Some(match plain {
                None => e,
                Some(a) => b.ins().bor(a, e),
            });
        }
        let ok = b.create_block();
        b.ins().brif(plain.unwrap(), ok, &[], slow, &[]);
        b.switch_to_block(ok);
        let v = self.vars[s as usize];
        let tag = tag_of(b, v);
        let (wi, nr, wr, nb, wb) = (b.create_block(), b.create_block(), b.create_block(), b.create_block(), b.create_block());
        let is_i = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_INT);
        b.ins().brif(is_i, wi, &[], nr, &[]);
        b.switch_to_block(wi);
        let x = b.use_var(v.i);
        let t = b.ins().iconst(types::I8, lay.t_int as i64);
        b.ins().store(MemFlagsData::trusted(), t, addr, 0);
        b.ins().store(MemFlagsData::trusted(), x, addr, 8);
        self.jump_next(b, ip);
        b.switch_to_block(nr);
        let is_r = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_REAL);
        b.ins().brif(is_r, wr, &[], nb, &[]);
        b.switch_to_block(wr);
        let x = b.use_var(v.r);
        let t = b.ins().iconst(types::I8, lay.t_real as i64);
        b.ins().store(MemFlagsData::trusted(), t, addr, 0);
        b.ins().store(MemFlagsData::trusted(), x, addr, 8);
        self.jump_next(b, ip);
        b.switch_to_block(nb);
        let is_b = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_BOOL);
        b.ins().brif(is_b, wb, &[], slow, &[]);
        b.switch_to_block(wb);
        let x = b.use_var(v.b);
        let t = b.ins().iconst(types::I8, lay.t_bool as i64);
        b.ins().store(MemFlagsData::trusted(), t, addr, 0);
        b.ins().store(MemFlagsData::trusted(), x, addr, 1);
        self.jump_next(b, ip);
    }

    /// Reads element `k` of a compact array of `kind` (data at `p`, bits at `bits`).
    fn read_packed(&self, b: &mut FunctionBuilder, ip: usize, d: u16, p: V, bits: V, kind: V, k: V, slow: Block) {
        let (ints, not_int, reals, bools) = (b.create_block(), b.create_block(), b.create_block(), b.create_block());
        let is0 = b.ins().icmp_imm_s(IntCC::Equal, kind, 0);
        b.ins().brif(is0, ints, &[], not_int, &[]);
        b.switch_to_block(not_int);
        let is1 = b.ins().icmp_imm_s(IntCC::Equal, kind, 1);
        b.ins().brif(is1, reals, &[], bools, &[]);
        for (blk, real) in [(ints, false), (reals, true)] {
            b.switch_to_block(blk);
            let has = self.has_bit(b, bits, k);
            let ok = b.create_block();
            b.ins().brif(has, ok, &[], slow, &[]);
            b.switch_to_block(ok);
            let off = b.ins().ishl_imm_s(k, 3);
            let a = b.ins().iadd(p, off);
            if real {
                let v = b.ins().load(types::F64, MemFlagsData::trusted(), a, 0);
                self.set_r(b, d, v);
            } else {
                let v = b.ins().load(types::I64, MemFlagsData::trusted(), a, 0);
                self.set_i(b, d, v);
            }
            self.jump_next(b, ip);
        }
        b.switch_to_block(bools);
        let a = b.ins().iadd(p, k);
        let v = b.ins().load(types::I8, MemFlagsData::trusted(), a, 0);
        let unset = b.ins().icmp_imm_s(IntCC::Equal, v, crate::vm::UNSET_BOOL as i64);
        let ok = b.create_block();
        b.ins().brif(unset, slow, &[], ok, &[]);
        b.switch_to_block(ok);
        self.set_b(b, d, v);
        self.jump_next(b, ip);
    }

    /// The "has a value" bit of element `k`.
    fn has_bit(&self, b: &mut FunctionBuilder, bits: V, k: V) -> V {
        // no bits: a full array
        let (look, done) = (b.create_block(), b.create_block());
        b.append_block_param(done, types::I64);
        let one = b.ins().iconst(types::I64, 1);
        b.ins().brif(bits, look, &[], done, &[one.into()]);
        b.switch_to_block(look);
        let x = self.has_bit_word(b, bits, k);
        b.ins().jump(done, &[x.into()]);
        b.switch_to_block(done);
        b.block_params(done)[0]
    }

    fn has_bit_word(&self, b: &mut FunctionBuilder, bits: V, k: V) -> V {
        let w = b.ins().ushr_imm_s(k, 6);
        let off = b.ins().ishl_imm_s(w, 3);
        let a = b.ins().iadd(bits, off);
        let word = b.ins().load(types::I64, MemFlagsData::trusted(), a, 0);
        let sh = b.ins().band_imm_s(k, 63);
        let x = b.ins().ushr(word, sh);
        b.ins().band_imm_s(x, 1)
    }

    /// Writes register `s` into element `k` of a compact array of `kind`.
    fn write_packed(&self, b: &mut FunctionBuilder, ip: usize, s: u16, p: V, bits: V, kind: V, k: V, slow: Block) {
        let v = self.vars[s as usize];
        let tag = tag_of(b, v);
        let (ints, not_int, reals, bools) = (b.create_block(), b.create_block(), b.create_block(), b.create_block());
        let is0 = b.ins().icmp_imm_s(IntCC::Equal, kind, 0);
        b.ins().brif(is0, ints, &[], not_int, &[]);
        b.switch_to_block(not_int);
        let is1 = b.ins().icmp_imm_s(IntCC::Equal, kind, 1);
        b.ins().brif(is1, reals, &[], bools, &[]);
        // ints: an int value
        b.switch_to_block(ints);
        let ok = b.create_block();
        let t_int = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_INT);
        b.ins().brif(t_int, ok, &[], slow, &[]);
        b.switch_to_block(ok);
        let x = b.use_var(v.i);
        let off = b.ins().ishl_imm_s(k, 3);
        let a = b.ins().iadd(p, off);
        b.ins().store(MemFlagsData::trusted(), x, a, 0);
        self.set_bit(b, bits, k);
        self.jump_next(b, ip);
        // floats: a float, or an int made a float
        b.switch_to_block(reals);
        let (from_r, not_r, from_i) = (b.create_block(), b.create_block(), b.create_block());
        let t_real = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_REAL);
        b.ins().brif(t_real, from_r, &[], not_r, &[]);
        b.switch_to_block(not_r);
        let t_int = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_INT);
        b.ins().brif(t_int, from_i, &[], slow, &[]);
        for (blk, conv) in [(from_r, false), (from_i, true)] {
            b.switch_to_block(blk);
            let x = if conv {
                let n = b.use_var(v.i);
                b.ins().fcvt_from_sint(types::F64, n)
            } else {
                b.use_var(v.r)
            };
            let off = b.ins().ishl_imm_s(k, 3);
            let a = b.ins().iadd(p, off);
            b.ins().store(MemFlagsData::trusted(), x, a, 0);
            self.set_bit(b, bits, k);
            self.jump_next(b, ip);
        }
        // bools: a bool
        b.switch_to_block(bools);
        let ok = b.create_block();
        let t_bool = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_BOOL);
        b.ins().brif(t_bool, ok, &[], slow, &[]);
        b.switch_to_block(ok);
        let x = b.use_var(v.b);
        let a = b.ins().iadd(p, k);
        b.ins().store(MemFlagsData::trusted(), x, a, 0);
        self.jump_next(b, ip);
    }

    fn set_bit(&self, b: &mut FunctionBuilder, bits: V, k: V) {
        // no bits: a full array, nothing to mark
        let (look, after) = (b.create_block(), b.create_block());
        b.ins().brif(bits, look, &[], after, &[]);
        b.switch_to_block(look);
        self.set_bit_word(b, bits, k);
        b.ins().jump(after, &[]);
        b.switch_to_block(after);
    }

    fn set_bit_word(&self, b: &mut FunctionBuilder, bits: V, k: V) {
        let w = b.ins().ushr_imm_s(k, 6);
        let off = b.ins().ishl_imm_s(w, 3);
        let a = b.ins().iadd(bits, off);
        let word = b.ins().load(types::I64, MemFlagsData::trusted(), a, 0);
        let sh = b.ins().band_imm_s(k, 63);
        let one = b.ins().iconst(types::I64, 1);
        let m = b.ins().ishl(one, sh);
        // written only when not set yet: refilling an array stores nothing here,
        // and 64 neighbours do not wait on each other through memory
        let has = b.ins().band(word, m);
        let (set, done) = (b.create_block(), b.create_block());
        b.ins().brif(has, done, &[], set, &[]);
        b.switch_to_block(set);
        let nw = b.ins().bor(word, m);
        b.ins().store(MemFlagsData::trusted(), nw, a, 0);
        b.ins().jump(done, &[]);
        b.switch_to_block(done);
    }

    /// Index `i` (1-based) in range 1..=n: its 0-based value, or a jump to `slow`.
    fn in_range(&self, b: &mut FunctionBuilder, i: V, n: V, slow: Block) -> V {
        let k = b.ins().iadd_imm_s(i, -1);
        let ok = b.ins().icmp(IntCC::UnsignedLessThan, k, n);
        let go = b.create_block();
        b.ins().brif(ok, go, &[], slow, &[]);
        b.switch_to_block(go);
        k
    }

    fn op(&self, b: &mut FunctionBuilder, ip: usize, op: Op) -> Result<(), String> {
        // elements read and written in place, the helpers only for the rest
        if let (Some(lay), true) = (self.layout, self.epoch.is_some()) {
            match op {
                Op::GetElem(d, c, i) | Op::SetElem(c, i, d) if self.spec.get(c as usize).is_some_and(|&k| k != SPEC_NONE) => {
                    // made for the kind seen: anything else goes back to the interpreter
                    let want = self.spec[c as usize] as i64;
                    let stop = self.stop(b, ip);
                    // the view made at entry is still valid here: no check, no call
                    let fresh = self.fresh.get(ip).copied().unwrap_or(false) && !self.clob.get(c as usize).copied().unwrap_or(true);
                    let (p, n) = if fresh {
                        let vv = self.views[c as usize];
                        (b.use_var(vv.ptr), b.use_var(vv.len))
                    } else {
                        let (p, n, kind) = self.view(b, c, false, stop)?;
                        let same = b.ins().icmp_imm_s(IntCC::Equal, kind, want);
                        let go = b.create_block();
                        b.ins().brif(same, go, &[], stop, &[]);
                        b.switch_to_block(go);
                        (p, n)
                    };
                    let bits = b.use_var(self.views[c as usize].bits);
                    let iv = self.i(b, i);
                    // in the check-free copy the chooser proved 1 <= counter <= length
                    let checked_already = self.in_fast.get().is_some_and(|h| {
                        let fl = &self.fastv[&h];
                        i == fl.var && fl.conts.contains(&c)
                    });
                    let k = if checked_already { b.ins().iadd_imm_s(iv, -1) } else { self.in_range(b, iv, n, stop) };
                    let get = matches!(op, Op::GetElem(..));
                    match (want, get) {
                        (2, true) => {
                            let a = b.ins().iadd(p, k);
                            let v = b.ins().load(types::I8, MemFlagsData::trusted(), a, 0);
                            let unset = b.ins().icmp_imm_s(IntCC::Equal, v, crate::vm::UNSET_BOOL as i64);
                            self.guard(b, ip, unset);
                            self.set_b(b, d, v);
                        }
                        (2, false) => {
                            let v = self.vars[d as usize];
                            let t = tag_of(b, v);
                            let bad = b.ins().icmp_imm_s(IntCC::NotEqual, t, TAG_BOOL);
                            self.guard(b, ip, bad);
                            let x = b.use_var(v.b);
                            let a = b.ins().iadd(p, k);
                            b.ins().store(MemFlagsData::trusted(), x, a, 0);
                        }
                        (_, true) => {
                            let has = self.has_bit(b, bits, k);
                            let none = b.ins().icmp_imm_s(IntCC::Equal, has, 0);
                            self.guard(b, ip, none);
                            let off = b.ins().ishl_imm_s(k, 3);
                            let a = b.ins().iadd(p, off);
                            if want == 1 {
                                let v = b.ins().load(types::F64, MemFlagsData::trusted(), a, 0);
                                self.set_r(b, d, v);
                            } else {
                                let v = b.ins().load(types::I64, MemFlagsData::trusted(), a, 0);
                                self.set_i(b, d, v);
                            }
                        }
                        (_, false) => {
                            let v = self.vars[d as usize];
                            let t = tag_of(b, v);
                            let off = b.ins().ishl_imm_s(k, 3);
                            let a = b.ins().iadd(p, off);
                            if want == 1 {
                                // a float, or an int made a float
                                let (fr, nr, fi, done) = (b.create_block(), b.create_block(), b.create_block(), b.create_block());
                                let is_r = b.ins().icmp_imm_s(IntCC::Equal, t, TAG_REAL);
                                b.ins().brif(is_r, fr, &[], nr, &[]);
                                b.switch_to_block(nr);
                                let is_i = b.ins().icmp_imm_s(IntCC::Equal, t, TAG_INT);
                                b.ins().brif(is_i, fi, &[], stop, &[]);
                                b.switch_to_block(fr);
                                let x = b.use_var(v.r);
                                b.ins().store(MemFlagsData::trusted(), x, a, 0);
                                b.ins().jump(done, &[]);
                                b.switch_to_block(fi);
                                let n2 = b.use_var(v.i);
                                let x = b.ins().fcvt_from_sint(types::F64, n2);
                                b.ins().store(MemFlagsData::trusted(), x, a, 0);
                                b.ins().jump(done, &[]);
                                b.switch_to_block(done);
                            } else {
                                let bad = b.ins().icmp_imm_s(IntCC::NotEqual, t, TAG_INT);
                                self.guard(b, ip, bad);
                                let x = self.i(b, d);
                                b.ins().store(MemFlagsData::trusted(), x, a, 0);
                            }
                            self.set_bit(b, bits, k);
                        }
                    }
                    self.jump_next(b, ip);
                    return Ok(());
                }
                Op::GetElem(d, c, i) | Op::SetElem(c, i, d) => {
                    let slow = b.create_block();
                    let (p, n, kind) = self.view(b, c, false, slow)?;
                    let bits = b.use_var(self.views[c as usize].bits);
                    let iv = self.i(b, i);
                    let k = self.in_range(b, iv, n, slow);
                    let (values, packed) = (b.create_block(), b.create_block());
                    let is_values = b.ins().icmp_imm_s(IntCC::Equal, kind, -1);
                    b.ins().brif(is_values, values, &[], packed, &[]);
                    b.switch_to_block(packed);
                    if matches!(op, Op::GetElem(..)) {
                        self.read_packed(b, ip, d, p, bits, kind, k, slow);
                    } else {
                        self.write_packed(b, ip, d, p, bits, kind, k, slow);
                    }
                    b.switch_to_block(values);
                    let a = self.elem_addr(b, p, k, lay);
                    if matches!(op, Op::GetElem(..)) {
                        self.read_elem(b, ip, d, a, lay, slow);
                    } else {
                        self.write_elem(b, ip, d, a, lay, slow);
                    }
                    b.switch_to_block(slow);
                    return self.slow_op(b, ip, op);
                }
                Op::GetElem2(d, c, i, j) | Op::SetElem2(c, i, j, d) => {
                    let slow = b.create_block();
                    let (p, rows, cols) = self.view(b, c, true, slow)?;
                    let (iv, jv) = (self.i(b, i), self.i(b, j));
                    let ki = self.in_range(b, iv, rows, slow);
                    let kj = self.in_range(b, jv, cols, slow);
                    let row = b.ins().imul(ki, cols);
                    let k = b.ins().iadd(row, kj);
                    let a = self.elem_addr(b, p, k, lay);
                    if matches!(op, Op::GetElem2(..)) {
                        self.read_elem(b, ip, d, a, lay, slow);
                    } else {
                        self.write_elem(b, ip, d, a, lay, slow);
                    }
                    b.switch_to_block(slow);
                    return self.slow_op(b, ip, op);
                }
                Op::Length(d, s) => {
                    let slow = b.create_block();
                    let (_, n, _) = self.view(b, s, false, slow)?;
                    self.set_i(b, d, n);
                    self.jump_next(b, ip);
                    b.switch_to_block(slow);
                    return self.slow_op(b, ip, op);
                }
                _ => {}
            }
        }
        self.slow_op(b, ip, op)
    }

    fn slow_op(&self, b: &mut FunctionBuilder, ip: usize, op: Op) -> Result<(), String> {
        match op {
            Op::LoadInt(d, n) => {
                let v = b.ins().iconst(types::I64, n);
                if let Some(l) = self.lconst {
                    let mut l = l.borrow_mut();
                    let e = &mut l[d as usize];
                    if e.0 && e.1 != n {
                        e.2 = true;
                    }
                    e.0 = true;
                    e.1 = n;
                }
                self.loading.set(true);
                self.set_i(b, d, v);
                self.loading.set(false);
                self.jump_next(b, ip);
            }
            Op::LoadReal(d, x) => {
                let v = b.ins().f64const(x);
                self.set_r(b, d, v);
                self.jump_next(b, ip);
            }
            Op::LoadBool(d, x) => {
                let v = b.ins().iconst(types::I8, x as i64);
                self.set_b(b, d, v);
                self.jump_next(b, ip);
            }
            Op::Move(d, s) => {
                let (vd, vs) = (self.vars[d as usize], self.vars[s as usize]);
                if self.leave.is_some() && vs.m & K_NONE == 0 {
                    // compiled code never puts a non-number there: if one came from
                    // the interpreter, it moves it
                    let t = tag_of(b, vs);
                    let none = b.ins().icmp_imm_s(IntCC::Equal, t, TAG_NONE);
                    self.guard(b, ip, none);
                } else if self.leave.is_some() {
                    // a value that is not a number moves in the interpreter's registers
                    let t = tag_of(b, vs);
                    let none = b.ins().icmp_imm_s(IntCC::Equal, t, TAG_NONE);
                    let (other, num, join) = (b.create_block(), b.create_block(), b.create_block());
                    b.ins().brif(none, other, &[], num, &[]);
                    b.switch_to_block(other);
                    let (dc, sc) = (self.konst(b, d as i64), self.konst(b, s as i64));
                    self.helper(b, "gw_move", &[dc, sc])?;
                    b.ins().jump(join, &[]);
                    b.switch_to_block(num);
                    b.ins().jump(join, &[]);
                    b.switch_to_block(join);
                }
                if let Some(mv) = self.moves {
                    mv.borrow_mut().push((d, s));
                }
                if let Some(mi) = self.move_ips {
                    mi.borrow_mut().push((ip, s));
                }
                self.saw(d, 0);
                // only the kinds both may hold: the tag says which one is live
                for (to, from, k) in [(vd.i, vs.i, K_INT), (vd.r, vs.r, K_REAL), (vd.b, vs.b, K_BOOL), (vd.t, vs.t, K_ALL)] {
                    if vd.m & vs.m & k != 0 {
                        let x = b.use_var(from);
                        b.def_var(to, x);
                    }
                }
                self.jump_next(b, ip);
            }
            Op::AddI(d, x, y) | Op::SubI(d, x, y) | Op::MulI(d, x, y) => {
                let (a, c) = (self.i(b, x), self.i(b, y));
                let which = match op {
                    Op::AddI(..) => 0,
                    Op::SubI(..) => 1,
                    _ => 2,
                };
                self.checked(b, ip, d, a, c, which);
            }
            Op::AddIImm(d, x, n) | Op::SubIImm(d, x, n) | Op::MulIImm(d, x, n) => {
                let a = self.i(b, x);
                let c = self.konst(b, n);
                let which = match op {
                    Op::AddIImm(..) => 0,
                    Op::SubIImm(..) => 1,
                    _ => 2,
                };
                self.checked(b, ip, d, a, c, which);
            }
            Op::DivI(d, x, y) | Op::ModI(d, x, y) => {
                let (a, c) = (self.i(b, x), self.i(b, y));
                self.divide(b, ip, d, a, c, matches!(op, Op::ModI(..)));
            }
            Op::DivIImm(d, x, n) | Op::ModIImm(d, x, n) => {
                let a = self.i(b, x);
                let c = self.konst(b, n);
                self.divide(b, ip, d, a, c, matches!(op, Op::ModIImm(..)));
            }
            Op::AddR(d, x, y) | Op::SubR(d, x, y) | Op::MulR(d, x, y) => {
                let (a, c) = (self.r(b, x), self.r(b, y));
                let v = match op {
                    Op::AddR(..) => b.ins().fadd(a, c),
                    Op::SubR(..) => b.ins().fsub(a, c),
                    _ => b.ins().fmul(a, c),
                };
                self.set_r(b, d, v);
                self.jump_next(b, ip);
            }
            Op::DivR(d, x, y) => {
                let (a, c) = (self.r(b, x), self.r(b, y));
                let z = b.ins().f64const(0.0);
                let is0 = b.ins().fcmp(FloatCC::Equal, c, z);
                self.guard(b, ip, is0);
                let v = b.ins().fdiv(a, c);
                self.set_r(b, d, v);
                self.jump_next(b, ip);
            }
            Op::NegI(d, s) => {
                let a = self.i(b, s);
                let m = b.ins().icmp_imm_s(IntCC::Equal, a, i64::MIN);
                self.guard(b, ip, m);
                let v = b.ins().ineg(a);
                self.set_i(b, d, v);
                self.jump_next(b, ip);
            }
            Op::NegR(d, s) => {
                let a = self.r(b, s);
                let v = b.ins().fneg(a);
                self.set_r(b, d, v);
                self.jump_next(b, ip);
            }
            Op::Not(d, s) => {
                let a = self.bo(b, s);
                let v = b.ins().bxor_imm_s(a, 1);
                self.set_b(b, d, v);
                self.jump_next(b, ip);
            }
            Op::ToReal(d, s) => {
                let a = self.i(b, s);
                let v = b.ins().fcvt_from_sint(types::F64, a);
                self.set_r(b, d, v);
                self.jump_next(b, ip);
            }
            Op::CmpI(c, d, x, y) => {
                let (a, e) = (self.i(b, x), self.i(b, y));
                let v = b.ins().icmp(int_cc(c), a, e);
                self.set_b(b, d, v);
                self.jump_next(b, ip);
            }
            Op::CmpIImm(c, d, x, n) => {
                let a = self.i(b, x);
                let v = b.ins().icmp_imm_s(int_cc(c), a, n);
                self.set_b(b, d, v);
                self.jump_next(b, ip);
            }
            Op::CmpR(c, d, x, y) => {
                let (a, e) = (self.r(b, x), self.r(b, y));
                let v = b.ins().fcmp(float_cc(c), a, e);
                self.set_b(b, d, v);
                self.jump_next(b, ip);
            }
            Op::CmpB(c, d, x, y) => {
                let (a, e) = (self.bo(b, x), self.bo(b, y));
                let v = b.ins().icmp(bool_cc(c), a, e);
                self.set_b(b, d, v);
                self.jump_next(b, ip);
            }
            Op::Jump(t) => {
                let blk = self.block(b, t as usize);
                b.ins().jump(blk, &[]);
            }
            Op::JumpIfFalse(c, t) => {
                let v = self.bo(b, c);
                let (n, tb) = (self.next(b, ip), self.block(b, t as usize));
                b.ins().brif(v, n, &[], tb, &[]);
            }
            Op::JumpIfTrue(c, t) => {
                let v = self.bo(b, c);
                let (n, tb) = (self.next(b, ip), self.block(b, t as usize));
                b.ins().brif(v, tb, &[], n, &[]);
            }
            Op::JumpUnlessI(c, x, y, t) => {
                let (a, e) = (self.i(b, x), self.i(b, y));
                let v = b.ins().icmp(int_cc(c), a, e);
                let (n, tb) = (self.next(b, ip), self.block(b, t as usize));
                b.ins().brif(v, n, &[], tb, &[]);
            }
            Op::JumpUnlessIImm(c, x, k, t) => {
                let a = self.i(b, x);
                let v = b.ins().icmp_imm_s(int_cc(c), a, k);
                let (n, tb) = (self.next(b, ip), self.block(b, t as usize));
                b.ins().brif(v, n, &[], tb, &[]);
            }
            Op::ForInit { var, end, step, exit } => {
                if let Some(&(_, flag)) = self.heads.get(&(ip + 1)) {
                    let z = b.ins().iconst(types::I8, 0);
                    b.def_var(flag, z);
                }
                let s = self.i(b, step);
                let s0 = b.ins().icmp_imm_s(IntCC::Equal, s, 0);
                self.guard(b, ip, s0);
                let (v, e) = (self.i(b, var), self.i(b, end));
                let up = b.ins().icmp_imm_s(IntCC::SignedGreaterThan, s, 0);
                let past_up = b.ins().icmp(IntCC::SignedGreaterThan, v, e);
                let down = b.ins().icmp_imm_s(IntCC::SignedLessThan, s, 0);
                let past_down = b.ins().icmp(IntCC::SignedLessThan, v, e);
                let a1 = b.ins().band(up, past_up);
                let a2 = b.ins().band(down, past_down);
                let skip = b.ins().bor(a1, a2);
                let (n, tb) = (self.next(b, ip), self.block(b, exit as usize));
                b.ins().brif(skip, tb, &[], n, &[]);
            }
            Op::ForStep { var, end, step, body } => {
                let (v, s) = (self.i(b, var), self.i(b, step));
                let (nv, of) = b.ins().sadd_overflow(v, s);
                let cont = b.create_block();
                let n = self.next(b, ip);
                // on overflow the counter is past any end: the loop is over
                b.ins().brif(of, n, &[], cont, &[]);
                b.switch_to_block(cont);
                self.set_i(b, var, nv);
                let e = self.i(b, end);
                let up = b.ins().icmp_imm_s(IntCC::SignedGreaterThan, s, 0);
                let in_up = b.ins().icmp(IntCC::SignedLessThanOrEqual, nv, e);
                let down = b.ins().icmp_imm_s(IntCC::SignedLessThan, s, 0);
                let in_down = b.ins().icmp(IntCC::SignedGreaterThanOrEqual, nv, e);
                let a1 = b.ins().band(up, in_up);
                let a2 = b.ins().band(down, in_down);
                let again = b.ins().bor(a1, a2);
                let bb = self.block(b, body as usize);
                b.ins().brif(again, bb, &[], n, &[]);
            }
            Op::Call { func, first, argc, dst } if !self.callee_refs.contains_key(&(func as usize)) => {
                if self.leave.is_none() {
                    return Err("internal: a call in a numeric function".into());
                }
                let n = (argc as u32).max(1) * 16;
                let ss = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, n, 3));
                let args = b.ins().stack_addr(types::I64, ss, 0);
                for k in 0..argc {
                    let v = self.vars[(first + k) as usize];
                    let (bits, tag) = (bits_of(b, v), tag_of(b, v));
                    b.ins().store(MemFlagsData::trusted(), bits, args, (k as i32) * 16);
                    b.ins().store(MemFlagsData::trusted(), tag, args, (k as i32) * 16 + 8);
                }
                let (fc, firstc, nc, dc, ipc) =
                    (self.konst(b, func as i64), self.konst(b, first as i64), self.konst(b, argc as i64), self.konst(b, dst as i64), self.konst(b, ip as i64));
                let out = self.out.ok_or("not in a function")?;
                let st = self.helper(b, "gw_call_body", &[fc, firstc, nc, dc, ipc, args, out])?;
                // 1: the interpreter makes this call; 2: the callee stopped and its
                // frame is the interpreter's now
                let (not1, done) = (b.create_block(), b.create_block());
                let is1 = b.ins().icmp_imm_s(IntCC::Equal, st, 1);
                let stop = self.stop(b, ip);
                b.ins().brif(is1, stop, &[], not1, &[]);
                b.switch_to_block(not1);
                let is2 = b.ins().icmp_imm_s(IntCC::Equal, st, 2);
                let active = self.exit_to(b, CALLEE_ACTIVE as usize);
                b.ins().brif(is2, active, &[], done, &[]);
                b.switch_to_block(done);
                if self.gives.get(func as usize).copied().unwrap_or(false) {
                    self.take_out(b, dst);
                }
                self.jump_next(b, ip);
            }
            Op::Call { func, first, argc, dst } => {
                let fref = *self.callee_refs.get(&(func as usize)).ok_or("internal: no callee")?;
                let mut args = Vec::with_capacity(argc as usize * 2 + 1);
                let mut any_none = None;
                for k in 0..argc {
                    let v = self.vars[(first + k) as usize];
                    let bits = bits_of(b, v);
                    let tag = tag_of(b, v);
                    let none = b.ins().icmp_imm_s(IntCC::Equal, tag, TAG_NONE);
                    any_none = Some(match any_none {
                        None => none,
                        Some(a) => b.ins().bor(a, none),
                    });
                    args.push(bits);
                    args.push(tag);
                }
                // an argument that is not a number: the interpreter makes this call
                if let Some(bad) = any_none {
                    self.guard(b, ip, bad);
                }
                let nb = b.ins().iadd_imm_s(self.budget, -1);
                args.push(nb);
                let call = b.ins().call(fref, &args);
                let res = b.inst_results(call).to_vec();
                let gave_up = b.ins().icmp_imm_s(IntCC::Equal, res[1], TAG_BAIL);
                self.guard(b, ip, gave_up);
                self.saw(dst, K_ALL);
                set_from_bits(b, self.vars[dst as usize], res[0], res[1]);
                self.jump_next(b, ip);
            }
            Op::Ret(s) if self.leave.is_none() => {
                let v = self.vars[s as usize];
                let bits = bits_of(b, v);
                let tag = tag_of(b, v);
                b.ins().return_(&[bits, tag]);
            }
            Op::RetNone if self.leave.is_none() => {
                b.ins().jump(self.bail, &[]);
            }
            Op::GetElem(d, c, i) => {
                let (dc, cc) = (self.konst(b, d as i64), self.konst(b, c as i64));
                let iv = self.i(b, i);
                let out = self.out.ok_or("not in a function")?;
                let st = self.helper(b, "gw_get_elem", &[dc, cc, iv, out])?;
                self.guard(b, ip, st);
                self.take_out(b, d);
                self.jump_next(b, ip);
            }
            Op::GetElem2(d, c, i, j) => {
                let (dc, cc) = (self.konst(b, d as i64), self.konst(b, c as i64));
                let (iv, jv) = (self.i(b, i), self.i(b, j));
                let out = self.out.ok_or("not in a function")?;
                let st = self.helper(b, "gw_get_elem2", &[dc, cc, iv, jv, out])?;
                self.guard(b, ip, st);
                self.take_out(b, d);
                self.jump_next(b, ip);
            }
            Op::SetElem(c, i, s) => {
                let cc = self.konst(b, c as i64);
                let iv = self.i(b, i);
                let v = self.vars[s as usize];
                let (bits, tag) = (bits_of(b, v), tag_of(b, v));
                let sc = self.konst(b, s as i64);
                let st = self.helper(b, "gw_set_elem", &[cc, iv, bits, tag, sc])?;
                self.guard(b, ip, st);
                self.jump_next(b, ip);
            }
            Op::SetElem2(c, i, j, s) => {
                let cc = self.konst(b, c as i64);
                let (iv, jv) = (self.i(b, i), self.i(b, j));
                let v = self.vars[s as usize];
                let (bits, tag) = (bits_of(b, v), tag_of(b, v));
                let sc = self.konst(b, s as i64);
                let st = self.helper(b, "gw_set_elem2", &[cc, iv, jv, bits, tag, sc])?;
                self.guard(b, ip, st);
                self.jump_next(b, ip);
            }
            Op::Length(d, s) => {
                let sc = self.konst(b, s as i64);
                let n = self.helper(b, "gw_length", &[sc])?;
                let bad = b.ins().icmp_imm_s(IntCC::SignedLessThan, n, 0);
                self.guard(b, ip, bad);
                self.set_i(b, d, n);
                self.jump_next(b, ip);
            }
            Op::Append(l, s) => {
                let lc = self.konst(b, l as i64);
                let v = self.vars[s as usize];
                let (bits, tag) = (bits_of(b, v), tag_of(b, v));
                let sc = self.konst(b, s as i64);
                let st = self.helper(b, "gw_append", &[lc, bits, tag, sc])?;
                self.guard(b, ip, st);
                self.jump_next(b, ip);
            }
            Op::GetGlobal(d, g) => {
                let (gc, dc) = (self.konst(b, g as i64), self.konst(b, d as i64));
                let out = self.out.ok_or("not in a function")?;
                let st = self.helper(b, "gw_get_global", &[gc, dc, out])?;
                self.guard(b, ip, st);
                self.take_out(b, d);
                self.jump_next(b, ip);
            }
            Op::SetGlobal(g, s) => {
                let gc = self.konst(b, g as i64);
                let v = self.vars[s as usize];
                let (bits, tag) = (bits_of(b, v), tag_of(b, v));
                let sc = self.konst(b, s as i64);
                let st = self.helper(b, "gw_set_global", &[gc, bits, tag, sc])?;
                self.guard(b, ip, st);
                self.jump_next(b, ip);
            }
            Op::LoadStr(d, k) => {
                let (dc, kc) = (self.konst(b, d as i64), self.konst(b, k as i64));
                self.helper(b, "gw_load_str", &[dc, kc])?;
                self.tag(b, d, TAG_NONE);
                self.jump_next(b, ip);
            }
            Op::NewList(d) => {
                let dc = self.konst(b, d as i64);
                self.helper(b, "gw_new_list", &[dc])?;
                self.tag(b, d, TAG_NONE);
                self.jump_next(b, ip);
            }
            Op::Concat(d, x, y) | Op::CharAt(d, x, y) => {
                let dc = self.konst(b, d as i64);
                let xc = self.konst(b, x as i64);
                let st = if matches!(op, Op::Concat(..)) {
                    let yc = self.konst(b, y as i64);
                    self.helper(b, "gw_concat", &[dc, xc, yc])?
                } else {
                    let iv = self.i(b, y);
                    self.helper(b, "gw_char_at", &[dc, xc, iv])?
                };
                self.guard(b, ip, st);
                self.tag(b, d, TAG_NONE);
                self.jump_next(b, ip);
            }
            Op::ToStr(d, s) => {
                let v = self.vars[s as usize];
                let (bits, tag) = (bits_of(b, v), tag_of(b, v));
                let (dc, sc) = (self.konst(b, d as i64), self.konst(b, s as i64));
                self.helper(b, "gw_to_str", &[dc, bits, tag, sc])?;
                self.tag(b, d, TAG_NONE);
                self.jump_next(b, ip);
            }
            Op::Print(s) => {
                let v = self.vars[s as usize];
                let (bits, tag) = (bits_of(b, v), tag_of(b, v));
                let sc = self.konst(b, s as i64);
                self.helper(b, "gw_print", &[bits, tag, sc])?;
                self.jump_next(b, ip);
            }
            Op::Builtin { id, first, argc, dst } => {
                // the arguments' bits and tags on the stack
                let ptr = types::I64;
                let n = (argc as u32).max(1) * 16;
                let ss = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, n, 3));
                let args = b.ins().stack_addr(ptr, ss, 0);
                for k in 0..argc {
                    let v = self.vars[(first + k) as usize];
                    let (bits, tag) = (bits_of(b, v), tag_of(b, v));
                    b.ins().store(MemFlagsData::trusted(), bits, args, (k as i32) * 16);
                    b.ins().store(MemFlagsData::trusted(), tag, args, (k as i32) * 16 + 8);
                }
                let (idc, fc, nc, dc) = (self.konst(b, id as i64), self.konst(b, first as i64), self.konst(b, argc as i64), self.konst(b, dst as i64));
                let out = self.out.ok_or("not in a function")?;
                let st = self.helper(b, "gw_builtin", &[idc, fc, nc, dc, args, out])?;
                self.guard(b, ip, st);
                let fun = crate::builtins::Builtin::from_id(id).is_function();
                if fun {
                    self.take_out(b, dst);
                }
                self.jump_next(b, ip);
            }
            Op::PrintLn => {
                self.helper(b, "gw_println", &[])?;
                self.jump_next(b, ip);
            }
            Op::CmpS(c, d, x, y) => {
                let cc = self.konst(
                    b,
                    match c {
                        Cmp::Eq => 0,
                        Cmp::NotEq => 1,
                        Cmp::Lt => 2,
                        Cmp::LtEq => 3,
                        Cmp::Gt => 4,
                        Cmp::GtEq => 5,
                    },
                );
                let (xc, yc) = (self.konst(b, x as i64), self.konst(b, y as i64));
                let r = self.helper(b, "gw_cmp_str", &[cc, xc, yc])?;
                let bad = b.ins().icmp_imm_s(IntCC::Equal, r, 2);
                self.guard(b, ip, bad);
                let v = b.ins().ireduce(types::I8, r);
                self.set_b(b, d, v);
                self.jump_next(b, ip);
            }
            other => return Err(format!("not compiled: {:?}", other)),
        }
        Ok(())
    }
}

/// The call budget for compiled code entered from the interpreter, with `frames`
/// interpreter frames already in use (the interpreter's own limit stays exact).
pub fn budget(frames: usize, max_depth: usize) -> i64 {
    (max_depth.saturating_sub(frames) as i64).min(NATIVE_DEPTH)
}
