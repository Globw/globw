//! Translates the typed program into register bytecode.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{BinOp, Shape};
use crate::bytecode::{Cmp, Function, Module, Op, Reg};
use crate::errors::Pos;
use crate::tir::*;

pub fn compile(p: &TProgram) -> Module {
    let mut strings = Strings::default();
    let functions = p
        .functions
        .iter()
        .enumerate()
        .map(|(k, f)| {
            let globals = if k == 0 { Some(&p.globals[..]) } else { None };
            FnCompiler::new(f, globals, &mut strings).finish()
        })
        .collect();
    let n_classes = crate::tir::class_count() as u16;
    let class_fields = (0..n_classes).map(crate::tir::class_field_kinds).collect();
    let vtable = (0..n_classes).map(crate::tir::class_vtable).collect();
    let class_parents = (0..n_classes).map(|c| crate::tir::class_parent(c).map_or(-1, |p| p as i32)).collect();
    Module {
        functions,
        strings: strings.list,
        globals: p.globals.iter().map(|s| s.name.clone()).collect(),
        class_fields,
        vtable,
        class_parents,
    }
}

#[derive(Default)]
struct Strings {
    list: Vec<Rc<str>>,
    index: HashMap<Rc<str>, u32>,
}

impl Strings {
    fn intern(&mut self, s: &Rc<str>) -> u32 {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(s.clone());
        self.index.insert(s.clone(), i);
        i
    }
}

fn alloc(shape: &Shape, dst: Reg) -> Option<Op> {
    match *shape {
        Shape::Array(n) => Some(Op::NewArray(dst, n as u32)),
        Shape::Matrix(r, c) => Some(Op::NewMatrix(dst, r as u32, c as u32)),
        Shape::List => Some(Op::NewList(dst)),
        Shape::Dict => Some(Op::NewDict(dst)),
        Shape::Scalar => None,
    }
}

/// The comparison to use when the two operands are swapped (`a < b` becomes `b > a`).
fn swap_cmp(c: Cmp) -> Cmp {
    match c {
        Cmp::Lt => Cmp::Gt,
        Cmp::Gt => Cmp::Lt,
        Cmp::LtEq => Cmp::GtEq,
        Cmp::GtEq => Cmp::LtEq,
        Cmp::Eq => Cmp::Eq,
        Cmp::NotEq => Cmp::NotEq,
    }
}

fn cmp_of(op: BinOp) -> Option<Cmp> {
    Some(match op {
        BinOp::Eq => Cmp::Eq,
        BinOp::NotEq => Cmp::NotEq,
        BinOp::Lt => Cmp::Lt,
        BinOp::LtEq => Cmp::LtEq,
        BinOp::Gt => Cmp::Gt,
        BinOp::GtEq => Cmp::GtEq,
        _ => return None,
    })
}

fn binary_op(op: BinOp, t: BaseType, d: Reg, a: Reg, b: Reg) -> Op {
    let cmp = |c: Cmp| match t {
        BaseType::Int => Op::CmpI(c, d, a, b),
        BaseType::Real => Op::CmpR(c, d, a, b),
        BaseType::Str => Op::CmpS(c, d, a, b),
        BaseType::Bool => Op::CmpB(c, d, a, b),
    };
    match (op, t) {
        (BinOp::Add, BaseType::Int) => Op::AddI(d, a, b),
        (BinOp::Add, BaseType::Real) => Op::AddR(d, a, b),
        (BinOp::Add, BaseType::Str) => Op::Concat(d, a, b),
        (BinOp::Sub, BaseType::Int) => Op::SubI(d, a, b),
        (BinOp::Sub, BaseType::Real) => Op::SubR(d, a, b),
        (BinOp::Mul, BaseType::Int) => Op::MulI(d, a, b),
        (BinOp::Mul, BaseType::Real) => Op::MulR(d, a, b),
        (BinOp::Div, _) => Op::DivR(d, a, b),
        (BinOp::IntDiv, _) => Op::DivI(d, a, b),
        (BinOp::Mod, _) => Op::ModI(d, a, b),
        (BinOp::Eq, _) => cmp(Cmp::Eq),
        (BinOp::NotEq, _) => cmp(Cmp::NotEq),
        (BinOp::Lt, _) => cmp(Cmp::Lt),
        (BinOp::LtEq, _) => cmp(Cmp::LtEq),
        (BinOp::Gt, _) => cmp(Cmp::Gt),
        (BinOp::GtEq, _) => cmp(Cmp::GtEq),
        (op, t) => panic!("internal error: no instruction for {:?} on {:?}", op, t),
    }
}

struct FnCompiler<'a> {
    f: &'a TFunction,
    /// only for the main program, which allocates global collections
    globals: Option<&'a [Slot]>,
    strings: &'a mut Strings,
    code: Vec<Op>,
    positions: Vec<Pos>,
    n_locals: Reg,
    next: Reg,
    max: Reg,
    /// jumps of `break` and `continue` to patch, one entry per enclosing loop
    loops: Vec<LoopJumps>,
}

#[derive(Default)]
struct LoopJumps {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl<'a> FnCompiler<'a> {
    fn new(f: &'a TFunction, globals: Option<&'a [Slot]>, strings: &'a mut Strings) -> FnCompiler<'a> {
        let n = f.locals.len() as Reg;
        FnCompiler {
            f,
            globals,
            strings,
            code: Vec::new(),
            positions: Vec::new(),
            n_locals: n,
            next: n,
            max: n,
            loops: Vec::new(),
        }
    }

    fn finish(mut self) -> Function {
        let start = Pos::default();
        if let Some(globals) = self.globals {
            for (g, slot) in globals.iter().enumerate() {
                let t = self.temp();
                if let Some(op) = alloc(&slot.shape, t) {
                    self.emit(op, start);
                    self.emit(Op::SetGlobal(g as u16, t), start);
                }
                self.next -= 1;
            }
        }
        let f = self.f;
        for (k, slot) in f.locals.iter().enumerate() {
            if !slot.is_param {
                if let Some(op) = alloc(&slot.shape, k as Reg) {
                    self.emit(op, start);
                }
            }
        }
        self.block(&f.body);
        self.emit(Op::RetNone, start);
        Function {
            name: f.name.clone(),
            n_params: f.n_params as u16,
            n_regs: self.max,
            code: self.code,
            positions: self.positions,
        }
    }

    fn emit(&mut self, op: Op, pos: Pos) -> usize {
        self.code.push(op);
        self.positions.push(pos);
        self.code.len() - 1
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    fn temp(&mut self) -> Reg {
        let r = self.next;
        self.next += 1;
        self.max = self.max.max(self.next);
        r
    }

    fn patch(&mut self, at: usize, target: u32) {
        match &mut self.code[at] {
            Op::Jump(t)
            | Op::JumpIfFalse(_, t)
            | Op::JumpIfTrue(_, t)
            | Op::JumpUnlessI(_, _, _, t)
            | Op::JumpUnlessIImm(_, _, _, t) => *t = target,
            Op::ForInit { exit, .. } => *exit = target,
            other => panic!("internal error: cannot patch {:?}", other),
        }
    }

    /// Evaluates `e` and returns the register holding the result: the
    /// variable's own register for a local read, otherwise a new temporary.
    fn operand(&mut self, e: &TExpr) -> Reg {
        if let TExprKind::Load(Place::Local(r)) = &e.kind {
            return *r;
        }
        let t = self.temp();
        self.expr_into(e, t);
        t
    }

    /// Emits a jump taken when `c` is false; returns its index for patching.
    fn jump_unless(&mut self, c: &TExpr) -> usize {
        if let TExprKind::Binary(op, BaseType::Int, a, b) = &c.kind {
            if let Some(cmp) = cmp_of(*op) {
                if let TExprKind::Int(n) = b.kind {
                    let x = self.operand(a);
                    return self.emit(Op::JumpUnlessIImm(cmp, x, n, 0), c.pos);
                }
                if let TExprKind::Int(n) = a.kind {
                    let y = self.operand(b);
                    return self.emit(Op::JumpUnlessIImm(swap_cmp(cmp), y, n, 0), c.pos);
                }
                let x = self.operand(a);
                let y = self.operand(b);
                return self.emit(Op::JumpUnlessI(cmp, x, y, 0), c.pos);
            }
        }
        let r = self.operand(c);
        self.emit(Op::JumpIfFalse(r, 0), c.pos)
    }

    /// Emits `d = a <op> b` with a constant operand folded into the
    /// instruction, avoiding a separate load. Returns false if no immediate
    /// form applies (then the caller emits the register form).
    fn try_int_imm(&mut self, op: BinOp, a: &TExpr, b: &TExpr, d: Reg, pos: Pos) -> bool {
        let lit = |e: &TExpr| if let TExprKind::Int(n) = e.kind { Some(n) } else { None };
        // the immediate must be the right operand; Add and Mul can swap, a
        // comparison swaps its direction, everything else needs b constant
        let (reg_expr, n, cmp) = if let Some(n) = lit(b) {
            (a, n, cmp_of(op))
        } else if let Some(n) = lit(a) {
            match cmp_of(op) {
                Some(c) => (b, n, Some(swap_cmp(c))),
                None if matches!(op, BinOp::Add | BinOp::Mul) => (b, n, None),
                None => return false,
            }
        } else {
            return false;
        };
        if matches!(op, BinOp::IntDiv | BinOp::Mod) && n == 0 {
            return false; // let the register form report division by zero
        }
        let r = self.operand(reg_expr);
        let instr = match (op, cmp) {
            (_, Some(c)) => Op::CmpIImm(c, d, r, n),
            (BinOp::Add, _) => Op::AddIImm(d, r, n),
            (BinOp::Sub, _) => Op::SubIImm(d, r, n),
            (BinOp::Mul, _) => Op::MulIImm(d, r, n),
            (BinOp::IntDiv, _) => Op::DivIImm(d, r, n),
            (BinOp::Mod, _) => Op::ModIImm(d, r, n),
            _ => return false,
        };
        self.emit(instr, pos);
        true
    }

    fn collection(&mut self, p: Place, pos: Pos) -> Reg {
        match p {
            Place::Local(r) => r,
            Place::Global(g) => {
                let t = self.temp();
                self.emit(Op::GetGlobal(t, g), pos);
                t
            }
        }
    }

    /// Evaluates call arguments into consecutive fresh registers; returns the first.
    fn args(&mut self, args: &[TExpr]) -> Reg {
        let first = self.next;
        for a in args {
            let t = self.temp();
            self.expr_into(a, t);
        }
        first
    }

    fn expr_into(&mut self, e: &TExpr, d: Reg) {
        let mark = self.next;
        let pos = e.pos;
        match &e.kind {
            TExprKind::Int(n) => {
                self.emit(Op::LoadInt(d, *n), pos);
            }
            TExprKind::Real(r) => {
                self.emit(Op::LoadReal(d, *r), pos);
            }
            TExprKind::Bool(b) => {
                self.emit(Op::LoadBool(d, *b), pos);
            }
            TExprKind::Str(s) => {
                let i = self.strings.intern(s);
                self.emit(Op::LoadStr(d, i), pos);
            }
            TExprKind::Load(Place::Local(r)) => {
                if *r != d {
                    self.emit(Op::Move(d, *r), pos);
                }
            }
            TExprKind::Load(Place::Global(g)) => {
                self.emit(Op::GetGlobal(d, *g), pos);
            }
            TExprKind::Elem(place, idx) => {
                let c = self.collection(*place, pos);
                let i = self.operand(&idx[0]);
                if idx.len() == 2 {
                    let j = self.operand(&idx[1]);
                    self.emit(Op::GetElem2(d, c, i, j), pos);
                } else {
                    self.emit(Op::GetElem(d, c, i), pos);
                }
            }
            TExprKind::Char(s, i) => {
                let s = self.operand(s);
                let i = self.operand(i);
                self.emit(Op::CharAt(d, s, i), pos);
            }
            TExprKind::ToReal(x) => {
                let r = self.operand(x);
                self.emit(Op::ToReal(d, r), pos);
            }
            TExprKind::ToStr(x) => {
                let r = self.operand(x);
                self.emit(Op::ToStr(d, r), pos);
            }
            TExprKind::Binary(op @ (BinOp::And | BinOp::Or), _, a, b) => self.logic(*op, a, b, d, pos),
            TExprKind::Binary(op, BaseType::Int, a, b) if self.try_int_imm(*op, a, b, d, pos) => {}
            TExprKind::Binary(op, t, a, b) => {
                let x = self.operand(a);
                let y = self.operand(b);
                self.emit(binary_op(*op, *t, d, x, y), pos);
            }
            TExprKind::Neg(t, x) => {
                let r = self.operand(x);
                self.emit(if *t == BaseType::Int { Op::NegI(d, r) } else { Op::NegR(d, r) }, pos);
            }
            TExprKind::Not(x) => {
                let r = self.operand(x);
                self.emit(Op::Not(d, r), pos);
            }
            TExprKind::Call(f, args) => {
                let first = self.args(args);
                self.emit(Op::Call { func: *f as u32, first, argc: args.len() as u16, dst: d }, pos);
            }
            TExprKind::VCall { slot, args } => {
                let first = self.args(args);
                self.emit(Op::CallMethod { slot: *slot as u32, first, argc: args.len() as u16, dst: d }, pos);
            }
            TExprKind::IsType(obj, cid) => {
                let r = self.operand(obj);
                self.emit(Op::IsClass(d, r, *cid as u32), pos);
            }
            TExprKind::Length(x) => {
                let r = self.operand(x);
                self.emit(Op::Length(d, r), pos);
            }
            TExprKind::Builtin(b, args) => {
                let first = self.args(args);
                self.emit(Op::Builtin { id: b.id(), first, argc: args.len() as u16, dst: d }, pos);
            }
            TExprKind::Index(c, k) => {
                let is_dict = matches!(c.ty, Type::Dict(..));
                let c = self.operand(c);
                let k = self.operand(k);
                self.emit(if is_dict { Op::GetKey(d, c, k) } else { Op::GetElem(d, c, k) }, pos);
            }
            TExprKind::GetField(obj, slot) => {
                let o = self.operand(obj);
                self.emit(Op::GetField(d, o, *slot as u16), pos);
            }
            TExprKind::New { class, ctor, args } => {
                let nfields = crate::tir::class_field_count(*class) as u32;
                let obj = self.temp();
                self.emit(Op::NewObject(obj, *class as u32, nfields), pos);
                if let Some(ctor) = ctor {
                    let first = self.next;
                    let selfreg = self.temp();
                    self.emit(Op::Move(selfreg, obj), pos);
                    for a in args {
                        let t = self.temp();
                        self.expr_into(a, t);
                    }
                    let throwaway = self.temp();
                    self.emit(Op::Call { func: *ctor as u32, first, argc: 1 + args.len() as u16, dst: throwaway }, pos);
                }
                self.emit(Op::Move(d, obj), pos);
            }
            TExprKind::ListLit(items) => {
                // build in a temporary: the items may read the variable being assigned
                let r = if d < self.n_locals { self.temp() } else { d };
                self.emit(Op::NewList(r), pos);
                for item in items {
                    let inner = self.next;
                    let v = self.operand(item);
                    self.emit(Op::Append(r, v), item.pos);
                    self.next = inner;
                }
                if r != d {
                    self.emit(Op::Move(d, r), pos);
                }
            }
        }
        self.next = mark;
    }

    fn logic(&mut self, op: BinOp, a: &TExpr, b: &TExpr, d: Reg, pos: Pos) {
        // writing straight into a variable would overwrite it before `b` reads it
        let r = if d < self.n_locals { self.temp() } else { d };
        self.expr_into(a, r);
        let jump = self.emit(if op == BinOp::And { Op::JumpIfFalse(r, 0) } else { Op::JumpIfTrue(r, 0) }, pos);
        self.expr_into(b, r);
        let end = self.here();
        self.patch(jump, end);
        if r != d {
            self.emit(Op::Move(d, r), pos);
        }
    }

    fn block(&mut self, body: &[TStmt]) {
        for s in body {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &TStmt) {
        let mark = self.next;
        match s {
            TStmt::Assign(t, v) => self.assign(t, v),
            TStmt::LoadModule { flag, func, pos } => {
                self.emit(Op::Ensure { flag: *flag, func: *func as u32 }, *pos);
            }
            TStmt::SetIndex(c, k, v, pos) => {
                let is_dict = matches!(c.ty, Type::Dict(..));
                let c = self.operand(c);
                let k = self.operand(k);
                let v = self.operand(v);
                self.emit(if is_dict { Op::SetKey(c, k, v) } else { Op::SetElem(c, k, v) }, *pos);
            }
            TStmt::SetField(obj, slot, v, pos) => {
                let o = self.operand(obj);
                let v = self.operand(v);
                self.emit(Op::SetField(o, *slot as u16, v), *pos);
            }
            TStmt::If(branches, other) => {
                let mut exits = Vec::new();
                for (c, body) in branches {
                    let skip = self.jump_unless(c);
                    self.next = mark;
                    self.block(body);
                    exits.push(self.emit(Op::Jump(0), c.pos));
                    let here = self.here();
                    self.patch(skip, here);
                }
                if let Some(body) = other {
                    self.block(body);
                }
                let end = self.here();
                for e in exits {
                    self.patch(e, end);
                }
            }
            TStmt::While(c, body) => {
                let top = self.here();
                let exit = self.jump_unless(c);
                self.next = mark;
                self.loop_body(body);
                self.emit(Op::Jump(top), c.pos);
                let end = self.here();
                self.patch(exit, end);
                self.close_loop(top, end);
            }
            TStmt::Repeat(body, c) => {
                let top = self.here();
                self.loop_body(body);
                let check = self.here();
                let r = self.operand(c);
                self.emit(Op::JumpIfTrue(r, top), c.pos);
                let end = self.here();
                self.close_loop(check, end);
            }
            TStmt::For { var, from, to, step, body, pos } => self.for_loop(*var, from, to, step.as_ref(), body, *pos),
            TStmt::ForEach { var, iter, kind, to_real, body, pos } => {
                self.for_each(*var, iter, *kind, *to_real, body, *pos)
            }
            TStmt::Break(pos) => {
                let j = self.emit(Op::Jump(0), *pos);
                self.loops.last_mut().expect("break outside a loop").breaks.push(j);
            }
            TStmt::Continue(pos) => {
                let j = self.emit(Op::Jump(0), *pos);
                self.loops.last_mut().expect("continue outside a loop").continues.push(j);
            }
            TStmt::Call(f, args, pos) => {
                let first = self.args(args);
                let dst = self.temp();
                self.emit(Op::Call { func: *f as u32, first, argc: args.len() as u16, dst }, *pos);
            }
            TStmt::VCall(slot, args, pos) => {
                let first = self.args(args);
                let dst = self.temp();
                self.emit(Op::CallMethod { slot: *slot as u32, first, argc: args.len() as u16, dst }, *pos);
            }
            TStmt::Output(args) => {
                for a in args {
                    let r = self.operand(a);
                    self.emit(Op::Print(r), a.pos);
                    self.next = mark;
                }
                self.emit(Op::PrintLn, Pos::default());
            }
            TStmt::Input(targets) => {
                for (t, ty) in targets {
                    match (t.place, t.indices.is_empty()) {
                        (Place::Local(r), true) => {
                            self.emit(Op::Read(r, *ty), t.pos);
                        }
                        _ => {
                            let r = self.temp();
                            self.emit(Op::Read(r, *ty), t.pos);
                            self.store(t, r);
                        }
                    }
                    self.next = mark;
                }
            }
            TStmt::Append(list, v) => {
                let l = self.operand(list);
                let r = self.operand(v);
                self.emit(Op::Append(l, r), list.pos);
            }
            TStmt::Remove(list, i, pos) => {
                let l = self.operand(list);
                let r = self.operand(i);
                self.emit(Op::Remove(l, r), *pos);
            }
            TStmt::Return(Some(v), pos) => {
                let r = self.operand(v);
                self.emit(Op::Ret(r), *pos);
            }
            TStmt::Return(None, pos) => {
                self.emit(Op::RetNone, *pos);
            }
            TStmt::Builtin(b, args, pos) => {
                let first = self.args(args);
                let dst = self.temp();
                self.emit(Op::Builtin { id: b.id(), first, argc: args.len() as u16, dst }, *pos);
            }
        }
        self.next = mark;
    }

    fn assign(&mut self, t: &TTarget, v: &TExpr) {
        match (t.place, t.indices.is_empty()) {
            (Place::Local(r), true) => self.expr_into(v, r),
            _ => {
                let r = self.operand(v);
                self.store(t, r);
            }
        }
    }

    fn store(&mut self, t: &TTarget, value: Reg) {
        match t.indices.len() {
            0 => match t.place {
                Place::Global(g) => {
                    self.emit(Op::SetGlobal(g, value), t.pos);
                }
                Place::Local(r) => {
                    if r != value {
                        self.emit(Op::Move(r, value), t.pos);
                    }
                }
            },
            1 => {
                let c = self.collection(t.place, t.pos);
                let i = self.operand(&t.indices[0]);
                self.emit(Op::SetElem(c, i, value), t.pos);
            }
            _ => {
                let c = self.collection(t.place, t.pos);
                let i = self.operand(&t.indices[0]);
                let j = self.operand(&t.indices[1]);
                self.emit(Op::SetElem2(c, i, j, value), t.pos);
            }
        }
    }

    fn for_loop(&mut self, var: Place, from: &TExpr, to: &TExpr, step: Option<&TExpr>, body: &[TStmt], pos: Pos) {
        // bounds and step are evaluated once, before the loop variable changes
        let start = self.temp();
        self.expr_into(from, start);
        let end = self.temp();
        self.expr_into(to, end);
        let step_reg = self.temp();
        match step {
            Some(s) => self.expr_into(s, step_reg),
            None => {
                self.emit(Op::LoadInt(step_reg, 1), pos);
            }
        }
        let counter = match var {
            Place::Local(r) => r,
            Place::Global(_) => self.temp(),
        };
        self.emit(Op::Move(counter, start), pos);
        let init = self.emit(Op::ForInit { var: counter, end, step: step_reg, exit: 0 }, pos);
        let top = self.here();
        if let Place::Global(g) = var {
            self.emit(Op::SetGlobal(g, counter), pos);
        }
        self.loop_body(body);
        let next = self.here();
        if let Place::Global(g) = var {
            self.emit(Op::GetGlobal(counter, g), pos);
        }
        self.emit(Op::ForStep { var: counter, end, step: step_reg, body: top }, pos);
        let exit = self.here();
        self.patch(init, exit);
        if let Place::Global(g) = var {
            // like a local counter, the global ends at the first value that failed the test
            self.emit(Op::SetGlobal(g, counter), pos);
        }
        let end = self.here();
        self.close_loop(next, end);
    }

    /// Compiles a loop body, collecting its `break` and `continue` jumps.
    fn loop_body(&mut self, body: &[TStmt]) {
        self.loops.push(LoopJumps::default());
        self.block(body);
    }

    /// Points the `continue` jumps of the innermost loop at `next` and its
    /// `break` jumps at `end`.
    fn close_loop(&mut self, next: u32, end: u32) {
        let jumps = self.loops.pop().expect("a loop was opened");
        for j in jumps.continues {
            self.patch(j, next);
        }
        for j in jumps.breaks {
            self.patch(j, end);
        }
    }

    /// `for(x in c)`: goes by position while the position is within the
    /// current length, so elements appended during the loop are visited too.
    /// Dictionaries go through a copy of their keys.
    fn for_each(&mut self, var: Place, iter: &TExpr, kind: IterKind, to_real: bool, body: &[TStmt], pos: Pos) {
        let c = self.temp();
        self.expr_into(iter, c);
        if kind == IterKind::Keys {
            self.emit(Op::Builtin { id: crate::builtins::Builtin::Keys.id(), first: c, argc: 1, dst: c }, pos);
        }
        let i = self.temp();
        self.emit(Op::LoadInt(i, 1), pos);
        let one = self.temp();
        self.emit(Op::LoadInt(one, 1), pos);
        let n = self.temp();
        let x = match var {
            Place::Local(r) if !to_real => r,
            _ => self.temp(),
        };
        let top = self.here();
        self.emit(Op::Length(n, c), pos);
        let exit = self.emit(Op::JumpUnlessI(Cmp::LtEq, i, n, 0), pos);
        self.emit(if kind == IterKind::Chars { Op::CharAt(x, c, i) } else { Op::GetElem(x, c, i) }, pos);
        match var {
            Place::Local(r) if to_real => {
                self.emit(Op::ToReal(r, x), pos);
            }
            Place::Local(_) => {}
            Place::Global(g) => {
                if to_real {
                    self.emit(Op::ToReal(x, x), pos);
                }
                self.emit(Op::SetGlobal(g, x), pos);
            }
        }
        self.loop_body(body);
        let next = self.here();
        self.emit(Op::AddI(i, i, one), pos);
        self.emit(Op::Jump(top), pos);
        let end = self.here();
        self.patch(exit, end);
        self.close_loop(next, end);
    }
}

#[cfg(test)]
mod tests {
    /// Compiles and runs `src`; a run-time error is appended to the output, rendered.
    fn run_src(src: &str, input: &str) -> String {
        let module = crate::compile(src)
            .unwrap_or_else(|errs| panic!("{}", errs.iter().map(|e| e.render(src)).collect::<String>()));
        let mut out = Vec::new();
        let result = crate::run(&module, &mut input.as_bytes(), &mut out);
        let mut text = String::from_utf8(out).unwrap();
        if let Err(e) = result {
            text.push_str(&e.render(src));
        }
        text
    }

    #[test]
    fn integer_conditions_use_fused_compare_and_jump() {
        let module = crate::compile("variables:\n    i as int\ni = 0\nwhile(i < 3)\n    i = i + 1\nend loop\n").unwrap();
        let code = &module.functions[0].code;
        use crate::bytecode::Op;
        assert!(code.iter().any(|op| matches!(op, Op::JumpUnlessI(..) | Op::JumpUnlessIImm(..))), "{:?}", code);
        assert!(!code.iter().any(|op| matches!(op, Op::CmpI(..) | Op::CmpIImm(..))), "{:?}", code);
    }

    #[test]
    fn rgb_random_and_time() {
        let src = "variables:\n    i, n as int\n    ok as bool\nok = true\nfor(i from 1 to 200)\n    n = random(1, 3)\n    if(n < 1 or n > 3)\n        ok = false\n    end if\nend loop\noutput(rgb(255, 128, 0), \" \", rgb(300, -5, 16), \" \", ok, \" \", time() >= 0)\n";
        assert_eq!(run_src(src, ""), "#ff8000 #ff0010 true true\n");
        assert!(run_src("variables:\n    n as int\nn = random(5, 1)\n", "")
            .contains("random: the minimum (5) is greater than the maximum (1)"));
    }

    #[test]
    fn math_functions() {
        let src = "output(sin(90), \" \", cos(0), \" \", sqrt(16), \" \", round(2.6), \" \", round(-2.5), \" \", round(7))
";
        assert_eq!(run_src(src, ""), "1.0 1.0 4.0 3 -3 7
");
        assert!(run_src("output(sqrt(-1))
", "").contains("sqrt of a negative number (-1.0)"));
    }

    #[test]
    fn fonts_work_without_a_window() {
        let src = "variables:
    a, b as int
font(\"pixel\", 16)
a = text_width(\"abc\")
font(\"pixel\", 32)
b = text_width(\"abc\")
output(a, \" \", b)
";
        assert_eq!(run_src(src, ""), "48 96
");
        assert!(run_src("font(\"Nope Font\", 12)
", "").contains("font \"Nope Font\" not found"));
    }

    #[test]
    fn standard_library() {
        let src = r#"variables:
    l as list of int
    w as list of string
    a as int[4]
    s as string
output(to_int("42") + 1, " ", to_int(3.9), " ", to_int(-3.9), " ", to_int(true))
output(to_float("3,5") * 2, " ", to_float(2), " ", to_text(7) + to_text(true))
output(char(65), code("a"), " ", to_hex(255), " ", to_binary(5))
output(absolute(-3), " ", absolute(-2.5), " ", minimum(3, 7), " ", maximum(2, 2.5))
output(power(2, 10), " ", floor(-2.5), " ", ceiling(2.1), " ", truncate(-2.9))
output(round_to(3.14159, 2), " ", round_to(logarithm(100, 10), 6), " ", exponential(0), " ", logarithm(1))
output(tan(45) > 0.99, " ", round_to(arctan2(1, 1), 6), " ", pi() > 3.14, " ", random_float() < 1)
s = "  Ciao Mondo  "
output("[", trim(s), "] ", uppercase("abc"), lowercase("ÀB"))
output(find("banana", "na"), " ", find("banana", "x"), " ", count("banana", "a"), " ", replace("banana", "a", "o"))
w = split("a,b,,c", ",")
output(length(w), " ", join(w, "-"), " ", starts_with("hello", "he"), " ", ends_with("hello", "x"), " ", substring("hello", 2, 4), " ", substring("hi", 0, 99), " ", contains("hello", "ell"))
output(length(split("abc", "")))
append(l, 5)
append(l, 2)
append(l, 9)
sort(l)
output(join(l, ","), " ", sum(l), " ", minimum(l), " ", maximum(l), " ", index_of(l, 9), " ", contains(l, 4))
insert(l, 1, 7)
reverse(l)
output(join(l, ","), " ", take_last(l), " ", length(l))
clear_list(l)
output(length(l))
a[1] = 4
a[2] = 1
a[3] = 3
a[4] = 2
sort(a)
output(join(a, " "))
write_file("target/globw_library_test.txt", "riga 1\n")
append_file("target/globw_library_test.txt", "riga 2\n")
w = read_lines("target/globw_library_test.txt")
output(file_exists("target/globw_library_test.txt"), " ", length(w), " ", w[2], " ", length(read_file("target/globw_library_test.txt")), " ", file_exists("target/nope.txt"))
output(length(today()), " ", length(now()))
wait(1)
output("before")
stop()
output("after")
"#;
        let expected = concat!(
            "43 3 -3 1\n",
            "7.0 2.0 7true\n",
            "A97 ff 101\n",
            "3 2.5 3 2.5\n",
            "1024.0 -3 3 -2\n",
            "3.14 2.0 1.0 0.0\n",
            "true 45.0 true true\n",
            "[Ciao Mondo] ABCàb\n",
            "3 0 3 bonono\n",
            "4 a-b--c true false ell hi true\n",
            "3\n",
            "2,5,9 16 2 9 3 false\n",
            "9,5,2,7 7 3\n",
            "0\n",
            "1 2 3 4\n",
            "true 2 riga 2 14 false\n",
            "10 8\n",
            "before\n",
        );
        assert_eq!(run_src(src, ""), expected);
    }

    #[test]
    fn standard_library_errors() {
        assert!(run_src("variables:\n    n as int\nn = to_int(\"abc\")\n", "").contains("to_int: \"abc\" is not a whole number"));
        assert!(run_src("variables:\n    l as list of int\n    n as int\nn = take_last(l)\n", "").contains("take_last: the list is empty"));
        assert!(run_src("variables:\n    l as list of int\n    n as int\nn = minimum(l)\n", "").contains("minimum: the list is empty"));
        assert!(run_src("variables:\n    s as string\ns = read_file(\"target/nope.txt\")\n", "").contains("cannot read the file \"target/nope.txt\""));
        assert!(run_src("variables:\n    l as list of int\ninsert(l, 3, 1)\n", "").contains("insert: the position must be from 1 to 1, here it is 3"));
    }

    #[test]
    fn inheritance_overrides_and_dynamic_dispatch() {
        let src = r#"variables:
    shapes as list of Shape
    i as int
class Shape
    function area() as float
        return 0
    end function
    function describe() as string
        return "area " + self.area()
    end function
end class
class Square is Shape
    variables:
        side as float
    procedure new(side as float)
        self.side = side
    end procedure
    function area() as float
        return self.side * self.side
    end function
end class
class Cube is Square
    function area() as float
        return 6 * parent.area()
    end function
    function describe() as string
        return "cube " + parent.describe()
    end function
end class
append(shapes, new Square(3))
append(shapes, new Cube(2))
append(shapes, new Shape())
for(i from 1 to length(shapes))
    output(shapes[i].describe(), "  square? ", shapes[i] is Square)
end loop
"#;
        assert_eq!(
            run_src(src, ""),
            "area 9.0  square? true
cube area 24.0  square? true
area 0.0  square? false
"
        );
    }

    #[test]
    fn immediate_integer_operands() {
        // a constant operand is folded into the instruction, with no LoadInt
        let module = crate::compile("variables:
    n as int
n = 7
output(n - 2, \" \", n mod 3, \" \", 3 * n)
").unwrap();
        use crate::bytecode::Op;
        let code = &module.functions[0].code;
        assert!(code.iter().any(|op| matches!(op, Op::SubIImm(..))), "{:?}", code);
        assert!(code.iter().any(|op| matches!(op, Op::ModIImm(..))), "{:?}", code);
        assert!(code.iter().any(|op| matches!(op, Op::MulIImm(..))), "{:?}", code);
        assert_eq!(run_src("output(7 - 2, \" \", (-7) div 2, \" \", (-7) mod 3, \" \", 2 - 7)
", ""), "5 -3 -1 -5
");
    }

    #[test]
    fn classes_objects_and_methods() {
        let src = r#"variables:
    a, b as Counter
    people as list of Person
    i as int
class Counter
    variables:
        value as int
    procedure new(start as int)
        self.value = start
    end procedure
    procedure add(n as int)
        self.value = self.value + n
    end procedure
    function get() as int
        return self.value
    end function
end class
class Person
    variables:
        name as string
        scores as list of int
    procedure new(name as string)
        self.name = name
    end procedure
    procedure note(s as int)
        append(self.scores, s)
    end procedure
    function best() as int
        return maximum(self.scores)
    end function
end class
a = new Counter(10)
b = a
a.add(5)
output(a.get(), " ", b.get())
b.add(100)
output(a.get(), " ", a.value)
append(people, new Person("Ann"))
append(people, new Person("Bob"))
people[1].note(7)
people[1].note(9)
people[2].note(6)
for(i from 1 to length(people))
    output(people[i].name, ": best ", people[i].best())
end loop
output(new Counter(3))
"#;
        assert_eq!(
            run_src(src, ""),
            "15 15
115 115
Ann: best 9
Bob: best 6
Counter(value=3)
"
        );
    }

    #[test]
    fn dictionaries_and_nested_lists() {
        let src = r#"variables:
    voti as dict of string to int
    g as list of list of int
    classi as dict of string to list of string
    k as list of string
voti["Anna"] = 8
voti["Luca"] = voti["Anna"] - 2
voti["Anna"] = 9
output(length(voti), " ", voti["Anna"], " ", voti["Luca"], " ", has_key(voti, "Marco"), " ", get(voti, "Marco", 0))
k = keys(voti)
output(join(k, ","), " ", join(values(voti), ","))
remove_key(voti, "Anna")
output(length(voti), " ", join(keys(voti), ","))
append(g, [1, 2, 3])
append(g, [])
append(g[2], 7)
g[1][2] = 9
output(length(g), " ", g[1][2], " ", length(g[2]), " ", g[2][1], " ", join(g[1], " "))
classi["3A"] = ["Anna", "Luca"]
append(classi["3A"], "Sara")
output(join(classi["3A"], ","), " ", length(classi))
output([10, 20, 30][2], " ", length([[1], [2, 3]]), " ", [1.5, 2][2])
output(voti["Zoe"])
"#;
        let out = run_src(src, "");
        let expected = concat!(
            "2 9 6 false 0\n",
            "Anna,Luca 9,6\n",
            "1 Luca\n",
            "2 9 1 7 1 9 3\n",
            "Anna,Luca,Sara 1\n",
            "20 2 2.0\n",
        );
        assert!(out.starts_with(expected), "{}", out);
        assert!(out.contains("key \"Zoe\" is not in the dictionary"), "{}", out);
    }

    #[test]
    fn graphics_need_a_window() {
        assert!(run_src("rect(0, 0, 10, 10, \"red\")\n", "").contains("there is no window: call window(...) first"));
        assert!(run_src("cube(0, 0, 5, 1, \"red\")\n", "").contains("there is no window: call window(...) first"));
        assert!(run_src("camera(0, 0, 0, 0, 0)\n", "").contains("there is no window: call window(...) first"));
        assert_eq!(run_src("output(open())\n", ""), "false\n");
    }

    #[test]
    fn arithmetic_and_output() {
        let src = "variables:\n    a, b as int\na = 7\nb = 2\noutput(a + b, \" \", a - b, \" \", a * b, \" \", a div b, \" \", a mod b, \" \", a / b)\n";
        assert_eq!(run_src(src, ""), "9 5 14 3 1 3.5\n");
    }

    #[test]
    fn input_numbers_are_added_not_joined() {
        let src = "variables:\n    a, b as int\n    p as float\ninput(a, b, p)\noutput(a + b, \" \", p * 2)\n";
        assert_eq!(run_src(src, "3\n4\n9,99\n"), "7 19.98\n");
    }

    #[test]
    fn if_else_chain() {
        let src = "variables:\n    v as int\ninput(v)\nif(v >= 8)\n    output(\"great\")\nelse if(v >= 6)\n    output(\"pass\")\nelse\n    output(\"fail\")\nend if\n";
        assert_eq!(run_src(src, "9\n"), "great\n");
        assert_eq!(run_src(src, "6\n"), "pass\n");
        assert_eq!(run_src(src, "2\n"), "fail\n");
    }

    #[test]
    fn while_and_repeat() {
        let src = "variables:\n    i, n as int\ni = 1\nwhile(i <= 3)\n    output(i)\n    i = i + 1\nend loop\nrepeat\n    input(n)\nwhile(n < 0)\noutput(\"ok \", n)\n";
        assert_eq!(run_src(src, "-1\n-5\n4\n"), "1\n2\n3\nok 4\n");
    }

    #[test]
    fn for_loops() {
        let src = "variables:\n    i as int\nfor(i from 10 to 0 step -5)\n    output(i)\nend loop\noutput(\"after: \", i)\n";
        assert_eq!(run_src(src, ""), "10\n5\n0\nafter: -5\n");
        let global = "global variables:\n    g as int\nvariables:\n    s as int\ns = 0\nfor(g from 1 to 4)\n    s = s + g\nend loop\noutput(s, \" \", g)\n";
        assert_eq!(run_src(global, ""), "10 5\n");
    }

    #[test]
    fn for_counter_changed_in_body_behaves_the_same_for_globals_and_locals() {
        let src = "global variables:\n    g as int\nvariables:\n    i as int\nfor(g from 1 to 5)\n    output(g)\n    g = g + 10\nend loop\noutput(\"after \", g)\nfor(i from 1 to 5)\n    output(i)\n    i = i + 10\nend loop\noutput(\"after \", i)\n";
        assert_eq!(run_src(src, ""), "1\nafter 12\n1\nafter 12\n");
    }

    #[test]
    fn functions_and_recursion() {
        let src = "variables:\n    r as int\nfunction fib(n as int) as int\n    if(n < 2)\n        return n\n    end if\n    return fib(n - 1) + fib(n - 2)\nend function\nr = fib(20)\noutput(r)\n";
        assert_eq!(run_src(src, ""), "6765\n");
    }

    #[test]
    fn int_argument_and_return_become_float() {
        let src = "function half(x as float) as float\n    return x / 2\nend function\nfunction one() as float\n    return 1\nend function\noutput(half(3), \" \", one())\n";
        assert_eq!(run_src(src, ""), "1.5 1.0\n");
    }

    #[test]
    fn arrays_by_reference_scalars_by_value() {
        let src = "variables:\n    v as int[3]\n    i, x as int\nprocedure fill(a as int[], y as int)\n    variables:\n        k as int\n    for(k from 1 to 3)\n        a[k] = k * k\n    end loop\n    y = 100\nend procedure\nx = 1\nfill(v, x)\nfor(i from 1 to 3)\n    output(v[i])\nend loop\noutput(x)\n";
        assert_eq!(run_src(src, ""), "1\n4\n9\n1\n");
    }

    #[test]
    fn lists_strings_and_matrices() {
        let src = "variables:\n    l as list of string\n    s as string\n    m as int[2, 3]\n    r, c, t as int\nappend(l, \"a\")\nappend(l, \"b\")\nappend(l, \"c\")\nremove(l, 2)\noutput(length(l), \" \", l[1], l[2])\ns = \"ciao\"\noutput(s[2], length(s))\nt = 0\nfor(r from 1 to 2)\n    for(c from 1 to 3)\n        m[r, c] = r * c\n        t = t + m[r, c]\n    end loop\nend loop\noutput(t)\n";
        assert_eq!(run_src(src, ""), "2 ac\ni4\n18\n");
    }

    #[test]
    fn short_circuit_and_logic_into_variable() {
        let src = "variables:\n    n as int\n    b, c as bool\nn = 0\nif(n != 0 and 10 div n > 1)\n    output(\"no\")\nelse\n    output(\"safe\")\nend if\nb = false\nc = true\nb = c and b\noutput(b)\n";
        assert_eq!(run_src(src, ""), "safe\nfalse\n");
    }

    #[test]
    fn reals_and_conversions() {
        let src = "variables:\n    x as float\nx = 1\nx = x / 4\noutput(x, \" \", 2 * 1.5, \" \", \"n=\" + 3 + true)\n";
        assert_eq!(run_src(src, ""), "0.25 3.0 n=3true\n");
    }

    #[test]
    fn runtime_errors_point_at_the_code() {
        assert_eq!(
            run_src("variables:\n    v as int[3]\nv[4] = 1\n", ""),
            "Error at line 3, column 1: index 4 is out of range: valid positions are 1 to 3\n    v[4] = 1\n    ^\n"
        );
        assert_eq!(
            run_src("variables:\n    x as int\nx = 0\noutput(1 / x)\n", ""),
            "Error at line 4, column 10: division by zero\n    output(1 / x)\n             ^\n"
        );
        let global = run_src("global variables:\n    g as int\nprocedure display()\n    output(g)\nend procedure\ndisplay()\ng = 1\n", "");
        assert!(global.starts_with("Error at line 4, column 12: variable \"g\" has no value yet"), "{}", global);
        assert!(run_src("variables:\n    n as int\ninput(n)\n", "9,99\n").contains("expected an int, read \"9,99\""));
    }
}
