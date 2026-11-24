//! The JIT: hot numeric code becomes machine code, through Cranelift.
//!
//! Two kinds of unit are compiled:
//! - **functions** whose instructions are all numeric (ints, reals, bools, jumps,
//!   calls to other such functions): they become native functions that call each
//!   other directly, with every value in a machine register;
//! - **loops** (`for` loops, from `ForInit` to their exit) made only of numeric
//!   instructions and calls to compiled functions, wherever they are (also in the
//!   main program, which as a whole prints and so cannot be compiled).
//!
//! Both are *pure*: they change nothing but registers. So when compiled code meets
//! anything it does not handle — an integer overflow, a division by zero, too many
//! nested calls — it gives up ("bails") without writing anything back, and the
//! interpreter runs that call or loop again from its start, producing the very same
//! error at the very same line (and `try` catches it as usual).
//!
//! Every register `r` is held in four Cranelift variables: its value as an int, as
//! a real, as a bool, and a tag saying which of the three it holds now. Typed
//! instructions read and write the typed variable directly (the bytecode is typed,
//! so a read as int always follows a write as int); `Move`, calls and the way back
//! to the interpreter go through the tag.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, AbiParam, Block, InstBuilder, MemFlagsData, Signature, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module as _};

use crate::bytecode::{Cmp, Function, Module, Op};
use crate::vm::Value;

pub const TAG_NONE: i64 = 0;
pub const TAG_INT: i64 = 1;
pub const TAG_REAL: i64 = 2;
pub const TAG_BOOL: i64 = 3;
/// returned as the tag of a result when compiled code gives up
pub const TAG_BAIL: i64 = 255;

/// How many compiled calls may nest before the interpreter takes over (the
/// machine stack is small; the interpreter keeps its frames on the heap).
const NATIVE_DEPTH: i64 = 2000;
/// A `for` loop is compiled when it is about to run at least this many times.
pub const HOT_LOOP: i64 = 64;
/// A function is compiled from its second call.
const HOT_CALLS: u32 = 2;
/// After this many give-ups a function or loop is left to the interpreter (a deep
/// recursion or an error inside a `try` would otherwise make it start over and over).
const MAX_BAILS: u32 = 8;

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

    pub fn value(self) -> Option<Value> {
        match self.tag {
            TAG_INT => Some(Value::Int(self.bits)),
            TAG_REAL => Some(Value::Real(f64::from_bits(self.bits as u64))),
            TAG_BOOL => Some(Value::Bool(self.bits != 0)),
            _ => None,
        }
    }
}

/// `fn(args: *const Slot, budget: i64, out: *mut Slot)`
type Trampoline = unsafe extern "C" fn(*const Slot, i64, *mut Slot);
/// `fn(regs: *mut Slot, budget: i64) -> i64` (0: done, 1: gave up)
type LoopFn = unsafe extern "C" fn(*mut Slot, i64) -> i64;

/// A compiled loop: its entry point and the registers it reads and writes.
pub struct LoopCode {
    pub entry: LoopFn,
    pub used: Vec<u16>,
    pub written: Vec<u16>,
    /// where the interpreter continues when the loop is over
    pub exit: usize,
}

pub struct Jit {
    /// GLOBW_JIT_LOG: report what is compiled and every give-up (for development)
    log: bool,
    module: JITModule,
    ctx: Context,
    fctx: FunctionBuilderContext,
    /// functions that can be compiled (all their instructions are handled)
    eligible: Vec<bool>,
    /// once the eligible functions are compiled: typed entry and trampoline
    funcs: Vec<Option<(FuncId, Trampoline)>>,
    compiled_funcs: bool,
    calls: Vec<u32>,
    func_bails: Vec<u32>,
    loops: HashMap<(usize, usize), Option<LoopCode>>,
    loop_bails: HashMap<(usize, usize), u32>,
    next_name: u32,
}

/// Is this instruction handled by compiled code? (`Call` is checked separately.)
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

fn jump_target(op: &Op) -> Option<usize> {
    Some(match *op {
        Op::Jump(t) | Op::JumpIfFalse(_, t) | Op::JumpIfTrue(_, t) => t as usize,
        Op::JumpUnlessI(_, _, _, t) | Op::JumpUnlessIImm(_, _, _, t) => t as usize,
        Op::ForInit { exit, .. } => exit as usize,
        Op::ForStep { body, .. } => body as usize,
        _ => return None,
    })
}

/// The registers an instruction reads and the one it writes.
fn regs_of(op: &Op, out: &mut Vec<u16>) {
    match *op {
        Op::LoadInt(d, _) | Op::LoadReal(d, _) | Op::LoadBool(d, _) => out.push(d),
        Op::Move(d, s) | Op::NegI(d, s) | Op::NegR(d, s) | Op::Not(d, s) | Op::ToReal(d, s) => out.extend([d, s]),
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
        _ => {}
    }
}

fn written_reg(op: &Op) -> Option<u16> {
    Some(match *op {
        Op::LoadInt(d, _) | Op::LoadReal(d, _) | Op::LoadBool(d, _) | Op::Move(d, _) => d,
        Op::AddI(d, ..) | Op::SubI(d, ..) | Op::MulI(d, ..) | Op::DivI(d, ..) | Op::ModI(d, ..) => d,
        Op::AddR(d, ..) | Op::SubR(d, ..) | Op::MulR(d, ..) | Op::DivR(d, ..) => d,
        Op::NegI(d, _) | Op::NegR(d, _) | Op::Not(d, _) | Op::ToReal(d, _) => d,
        Op::CmpI(_, d, ..) | Op::CmpR(_, d, ..) | Op::CmpB(_, d, ..) | Op::CmpIImm(_, d, ..) => d,
        Op::AddIImm(d, ..) | Op::SubIImm(d, ..) | Op::MulIImm(d, ..) | Op::DivIImm(d, ..) | Op::ModIImm(d, ..) => d,
        Op::ForStep { var, .. } => var,
        Op::Call { dst, .. } => dst,
        _ => return None,
    })
}

/// The registers of one compiled unit, as Cranelift variables.
#[derive(Clone, Copy)]
struct RegVars {
    i: Variable,
    r: Variable,
    b: Variable,
    t: Variable,
}

impl Jit {
    /// `None` when this machine has no Cranelift backend.
    pub fn new(module: &Module) -> Option<Jit> {
        let mut flags = settings::builder();
        flags.set("opt_level", "speed").ok()?;
        flags.set("use_colocated_libcalls", "false").ok()?;
        flags.set("is_pic", "false").ok()?;
        let isa = cranelift_native::builder().ok()?.finish(settings::Flags::new(flags)).ok()?;
        let jm = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
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
            eligible,
            funcs: (0..n).map(|_| None).collect(),
            compiled_funcs: false,
            calls: vec![0; n],
            func_bails: vec![0; n],
            loops: HashMap::new(),
            loop_bails: HashMap::new(),
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
            if self.build_function(m, f, id, &ids).is_err() {
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

    /// Compiled code for the loop at (`f`, `ip`) gave up (the interpreter runs it instead).
    pub fn loop_bailed(&mut self, f: usize, ip: usize) {
        *self.loop_bails.entry((f, ip)).or_insert(0) += 1;
        if self.log {
            eprintln!("[jit] loop at {}:{} gave up", f, ip);
        }
    }

    /// The compiled code of the loop starting at `ForInit` instruction `ip` of
    /// function `f`, compiled on first use; `None` if it cannot be compiled.
    pub fn loop_at(&mut self, m: &Module, f: usize, ip: usize) -> Option<&LoopCode> {
        if self.loop_bails.get(&(f, ip)).copied().unwrap_or(0) >= MAX_BAILS {
            return None;
        }
        if !self.loops.contains_key(&(f, ip)) {
            let code = self.compile_loop(m, f, ip);
            if self.log {
                eprintln!("[jit] loop at {}:{} {}", f, ip, if code.is_some() { "compiled" } else { "not compilable" });
            }
            self.loops.insert((f, ip), code);
        }
        self.loops.get(&(f, ip)).and_then(|c| c.as_ref())
    }

    fn compile_loop(&mut self, m: &Module, f: usize, start: usize) -> Option<LoopCode> {
        let func = &m.functions[f];
        let Op::ForInit { exit, .. } = func.code[start] else { return None };
        let exit = exit as usize;
        if exit <= start || exit > func.code.len() {
            return None;
        }
        let mut needs_funcs = false;
        for op in &func.code[start..exit] {
            match op {
                Op::Call { func: c, .. } => {
                    if !self.eligible.get(*c as usize).copied().unwrap_or(false) {
                        return None;
                    }
                    needs_funcs = true;
                }
                op if plain_numeric(op) => {}
                _ => return None,
            }
            if let Some(t) = jump_target(op) {
                if t < start || t > exit {
                    return None;
                }
            }
        }
        if needs_funcs && !self.compile_functions(m) {
            return None;
        }
        let mut used = Vec::new();
        let mut written = Vec::new();
        for op in &func.code[start..exit] {
            regs_of(op, &mut used);
            if let Some(w) = written_reg(op) {
                written.push(w);
            }
        }
        used.sort_unstable();
        used.dedup();
        written.sort_unstable();
        written.dedup();
        let ids: Vec<Option<FuncId>> = self.funcs.iter().map(|x| x.map(|(id, _)| id)).collect();
        let id = self.build_loop(func, start, exit, &used, &written, &ids).ok()?;
        self.module.finalize_definitions().ok()?;
        let p = self.module.get_finalized_function(id);
        // SAFETY: built with the LoopFn signature
        let entry = unsafe { std::mem::transmute::<*const u8, LoopFn>(p) };
        Some(LoopCode { entry, used, written, exit })
    }

    // ------------------------------------------------------------------ codegen

    fn build_function(&mut self, m: &Module, f: &Function, id: FuncId, ids: &[Option<FuncId>]) -> Result<(), String> {
        let _ = m;
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
                    let bits = params[2 * r];
                    let tag = params[2 * r + 1];
                    set_from_bits(&mut b, *v, bits, tag);
                } else {
                    b.def_var(v.i, zero);
                    b.def_var(v.r, zf);
                    b.def_var(v.b, zb);
                    b.def_var(v.t, zero);
                }
            }
            let blocks: Vec<Block> = (0..code.len()).map(|_| b.create_block()).collect();
            b.ins().jump(blocks[0], &[]);
            let g = Gen { vars: &vars, blocks: &blocks, lo: 0, exit_block: None, bail, budget, callee_refs: &callee_refs, function: true };
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

    fn build_loop(&mut self, f: &Function, start: usize, exit: usize, used: &[u16], written: &[u16], ids: &[Option<FuncId>]) -> Result<FuncId, String> {
        let ptr = self.module.isa().pointer_type();
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(ptr));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let name = self.fresh_name("loop");
        let id = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| format!("{:?}", e))?;
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = sig;
        self.ctx.func.name = UserFuncName::user(0, id.as_u32());
        let code = &f.code[..];
        let mut callee_refs = HashMap::new();
        let fc = self.module.isa().frontend_config();
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            for (i, fid) in ids.iter().enumerate() {
                if let Some(fid) = fid {
                    if code[start..exit].iter().any(|op| matches!(op, Op::Call { func, .. } if *func as usize == i)) {
                        callee_refs.insert(i, self.module.declare_func_in_func(*fid, b.func));
                    }
                }
            }
            let n_regs = f.n_regs as usize;
            let vars: Vec<RegVars> = (0..n_regs).map(|_| declare_reg(&mut b)).collect();
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let p = b.block_params(entry).to_vec();
            let (slots, budget) = (p[0], p[1]);
            let zero = b.ins().iconst(types::I64, 0);
            let zf = b.ins().f64const(0.0);
            let zb = b.ins().iconst(types::I8, 0);
            for (r, v) in vars.iter().enumerate() {
                if used.binary_search(&(r as u16)).is_ok() {
                    let bits = b.ins().load(types::I64, MemFlagsData::trusted(), slots, (r * 16) as i32);
                    let tag = b.ins().load(types::I64, MemFlagsData::trusted(), slots, (r * 16 + 8) as i32);
                    set_from_bits(&mut b, *v, bits, tag);
                } else {
                    b.def_var(v.i, zero);
                    b.def_var(v.r, zf);
                    b.def_var(v.b, zb);
                    b.def_var(v.t, zero);
                }
            }
            let blocks: Vec<Block> = (start..exit).map(|_| b.create_block()).collect();
            let exit_block = b.create_block();
            let bail = b.create_block();
            b.ins().jump(blocks[0], &[]);
            let g = Gen { vars: &vars, blocks: &blocks, lo: start, exit_block: Some((exit, exit_block)), bail, budget, callee_refs: &callee_refs, function: false };
            for ip in start..exit {
                b.switch_to_block(blocks[ip - start]);
                g.op(&mut b, ip, code[ip])?;
            }
            // the loop is over: hand the written registers back
            b.switch_to_block(exit_block);
            for &r in written {
                let v = vars[r as usize];
                let bits = bits_of(&mut b, v);
                let tag = b.use_var(v.t);
                b.ins().store(MemFlagsData::trusted(), bits, slots, (r as usize * 16) as i32);
                b.ins().store(MemFlagsData::trusted(), tag, slots, (r as usize * 16 + 8) as i32);
            }
            let done = b.ins().iconst(types::I64, 0);
            b.ins().return_(&[done]);
            b.switch_to_block(bail);
            let gave_up = b.ins().iconst(types::I64, 1);
            b.ins().return_(&[gave_up]);
            b.seal_all_blocks();
            b.finalize(fc);
        }
        self.module.define_function(id, &mut self.ctx).map_err(|e| format!("{:?}", e))?;
        Ok(id)
    }
}

fn declare_reg(b: &mut FunctionBuilder) -> RegVars {
    RegVars { i: b.declare_var(types::I64), r: b.declare_var(types::F64), b: b.declare_var(types::I8), t: b.declare_var(types::I64) }
}

/// Sets all of a register's variables from raw bits and a tag.
fn set_from_bits(b: &mut FunctionBuilder, v: RegVars, bits: cranelift_codegen::ir::Value, tag: cranelift_codegen::ir::Value) {
    b.def_var(v.i, bits);
    let r = b.ins().bitcast(types::F64, MemFlagsData::new(), bits);
    b.def_var(v.r, r);
    let bo = b.ins().ireduce(types::I8, bits);
    b.def_var(v.b, bo);
    b.def_var(v.t, tag);
}

/// The register's current value as raw bits, chosen by its tag.
fn bits_of(b: &mut FunctionBuilder, v: RegVars) -> cranelift_codegen::ir::Value {
    let tag = b.use_var(v.t);
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

/// Translates instructions of one unit.
struct Gen<'a> {
    vars: &'a [RegVars],
    blocks: &'a [Block],
    /// the index of the unit's first instruction
    lo: usize,
    /// a loop: its exit instruction and the block that hands the registers back
    exit_block: Option<(usize, Block)>,
    bail: Block,
    budget: cranelift_codegen::ir::Value,
    callee_refs: &'a HashMap<usize, cranelift_codegen::ir::FuncRef>,
    function: bool,
}

type V = cranelift_codegen::ir::Value;

impl Gen<'_> {
    fn block(&self, ip: usize) -> Block {
        if let Some((exit, blk)) = self.exit_block {
            if ip == exit {
                return blk;
            }
        }
        self.blocks[ip - self.lo]
    }

    fn next(&self, ip: usize) -> Block {
        self.block(ip + 1)
    }

    fn i(&self, b: &mut FunctionBuilder, r: u16) -> V {
        b.use_var(self.vars[r as usize].i)
    }
    fn r(&self, b: &mut FunctionBuilder, r: u16) -> V {
        b.use_var(self.vars[r as usize].r)
    }
    fn bo(&self, b: &mut FunctionBuilder, r: u16) -> V {
        b.use_var(self.vars[r as usize].b)
    }
    fn tag(&self, b: &mut FunctionBuilder, r: u16, t: i64) {
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

    /// dst = checked `op`: gives up on overflow
    fn checked(&self, b: &mut FunctionBuilder, ip: usize, d: u16, x: V, y: V, which: u8) {
        let (v, of) = match which {
            0 => b.ins().sadd_overflow(x, y),
            1 => b.ins().ssub_overflow(x, y),
            _ => b.ins().smul_overflow(x, y),
        };
        self.set_i(b, d, v);
        let next = self.next(ip);
        b.ins().brif(of, self.bail, &[], next, &[]);
    }

    /// dst = x / y or x mod y: gives up on y == 0 and on MIN / -1
    fn divide(&self, b: &mut FunctionBuilder, ip: usize, d: u16, x: V, y: V, rem: bool) {
        let y0 = b.ins().icmp_imm_s(IntCC::Equal, y, 0);
        let xm = b.ins().icmp_imm_s(IntCC::Equal, x, i64::MIN);
        let ym = b.ins().icmp_imm_s(IntCC::Equal, y, -1);
        let both = b.ins().band(xm, ym);
        let bad = b.ins().bor(y0, both);
        let ok = b.create_block();
        b.ins().brif(bad, self.bail, &[], ok, &[]);
        b.switch_to_block(ok);
        let v = if rem { b.ins().srem(x, y) } else { b.ins().sdiv(x, y) };
        self.set_i(b, d, v);
        let next = self.next(ip);
        b.ins().jump(next, &[]);
    }

    fn op(&self, b: &mut FunctionBuilder, ip: usize, op: Op) -> Result<(), String> {
        let next = |s: &Self| s.next(ip);
        match op {
            Op::LoadInt(d, n) => {
                let v = b.ins().iconst(types::I64, n);
                self.set_i(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::LoadReal(d, x) => {
                let v = b.ins().f64const(x);
                self.set_r(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::LoadBool(d, x) => {
                let v = b.ins().iconst(types::I8, x as i64);
                self.set_b(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::Move(d, s) => {
                let (vd, vs) = (self.vars[d as usize], self.vars[s as usize]);
                for (to, from) in [(vd.i, vs.i), (vd.r, vs.r), (vd.b, vs.b), (vd.t, vs.t)] {
                    let x = b.use_var(from);
                    b.def_var(to, x);
                }
                b.ins().jump(next(self), &[]);
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
                let c = b.ins().iconst(types::I64, n);
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
                let c = b.ins().iconst(types::I64, n);
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
                b.ins().jump(next(self), &[]);
            }
            Op::DivR(d, x, y) => {
                let (a, c) = (self.r(b, x), self.r(b, y));
                let z = b.ins().f64const(0.0);
                let is0 = b.ins().fcmp(FloatCC::Equal, c, z);
                let v = b.ins().fdiv(a, c);
                self.set_r(b, d, v);
                let n = next(self);
                b.ins().brif(is0, self.bail, &[], n, &[]);
            }
            Op::NegI(d, s) => {
                let a = self.i(b, s);
                let m = b.ins().icmp_imm_s(IntCC::Equal, a, i64::MIN);
                let v = b.ins().ineg(a);
                self.set_i(b, d, v);
                let n = next(self);
                b.ins().brif(m, self.bail, &[], n, &[]);
            }
            Op::NegR(d, s) => {
                let a = self.r(b, s);
                let v = b.ins().fneg(a);
                self.set_r(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::Not(d, s) => {
                let a = self.bo(b, s);
                let v = b.ins().bxor_imm_s(a, 1);
                self.set_b(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::ToReal(d, s) => {
                let a = self.i(b, s);
                let v = b.ins().fcvt_from_sint(types::F64, a);
                self.set_r(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::CmpI(c, d, x, y) => {
                let (a, e) = (self.i(b, x), self.i(b, y));
                let v = b.ins().icmp(int_cc(c), a, e);
                self.set_b(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::CmpIImm(c, d, x, n) => {
                let a = self.i(b, x);
                let v = b.ins().icmp_imm_s(int_cc(c), a, n);
                self.set_b(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::CmpR(c, d, x, y) => {
                let (a, e) = (self.r(b, x), self.r(b, y));
                let v = b.ins().fcmp(float_cc(c), a, e);
                self.set_b(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::CmpB(c, d, x, y) => {
                let (a, e) = (self.bo(b, x), self.bo(b, y));
                let v = b.ins().icmp(bool_cc(c), a, e);
                self.set_b(b, d, v);
                b.ins().jump(next(self), &[]);
            }
            Op::Jump(t) => {
                let blk = self.block(t as usize);
                b.ins().jump(blk, &[]);
            }
            Op::JumpIfFalse(c, t) => {
                let v = self.bo(b, c);
                let (n, tb) = (next(self), self.block(t as usize));
                b.ins().brif(v, n, &[], tb, &[]);
            }
            Op::JumpIfTrue(c, t) => {
                let v = self.bo(b, c);
                let (n, tb) = (next(self), self.block(t as usize));
                b.ins().brif(v, tb, &[], n, &[]);
            }
            Op::JumpUnlessI(c, x, y, t) => {
                let (a, e) = (self.i(b, x), self.i(b, y));
                let v = b.ins().icmp(int_cc(c), a, e);
                let (n, tb) = (next(self), self.block(t as usize));
                b.ins().brif(v, n, &[], tb, &[]);
            }
            Op::JumpUnlessIImm(c, x, k, t) => {
                let a = self.i(b, x);
                let v = b.ins().icmp_imm_s(int_cc(c), a, k);
                let (n, tb) = (next(self), self.block(t as usize));
                b.ins().brif(v, n, &[], tb, &[]);
            }
            Op::ForInit { var, end, step, exit } => {
                let s = self.i(b, step);
                let s0 = b.ins().icmp_imm_s(IntCC::Equal, s, 0);
                let go = b.create_block();
                b.ins().brif(s0, self.bail, &[], go, &[]);
                b.switch_to_block(go);
                let (v, e) = (self.i(b, var), self.i(b, end));
                let up = b.ins().icmp_imm_s(IntCC::SignedGreaterThan, s, 0);
                let past_up = b.ins().icmp(IntCC::SignedGreaterThan, v, e);
                let down = b.ins().icmp_imm_s(IntCC::SignedLessThan, s, 0);
                let past_down = b.ins().icmp(IntCC::SignedLessThan, v, e);
                let a1 = b.ins().band(up, past_up);
                let a2 = b.ins().band(down, past_down);
                let skip = b.ins().bor(a1, a2);
                let (n, tb) = (next(self), self.block(exit as usize));
                b.ins().brif(skip, tb, &[], n, &[]);
            }
            Op::ForStep { var, end, step, body } => {
                let (v, s) = (self.i(b, var), self.i(b, step));
                let (nv, of) = b.ins().sadd_overflow(v, s);
                let cont = b.create_block();
                let n = next(self);
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
                let bb = self.block(body as usize);
                b.ins().brif(again, bb, &[], n, &[]);
            }
            Op::Call { func, first, argc, dst } => {
                let fref = *self.callee_refs.get(&(func as usize)).ok_or("internal: no callee")?;
                let mut args = Vec::with_capacity(argc as usize * 2 + 1);
                for k in 0..argc {
                    let v = self.vars[(first + k) as usize];
                    let bits = bits_of(b, v);
                    let tag = b.use_var(v.t);
                    args.push(bits);
                    args.push(tag);
                }
                let nb = b.ins().iadd_imm_s(self.budget, -1);
                args.push(nb);
                let call = b.ins().call(fref, &args);
                let res = b.inst_results(call).to_vec();
                let gave_up = b.ins().icmp_imm_s(IntCC::Equal, res[1], TAG_BAIL);
                let ok = b.create_block();
                b.ins().brif(gave_up, self.bail, &[], ok, &[]);
                b.switch_to_block(ok);
                set_from_bits(b, self.vars[dst as usize], res[0], res[1]);
                b.ins().jump(next(self), &[]);
            }
            Op::Ret(s) if self.function => {
                let v = self.vars[s as usize];
                let bits = bits_of(b, v);
                let tag = b.use_var(v.t);
                b.ins().return_(&[bits, tag]);
            }
            Op::RetNone if self.function => {
                b.ins().jump(self.bail, &[]);
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
