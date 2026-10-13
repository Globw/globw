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
    Module { functions, strings: strings.list, globals: p.globals.iter().map(|s| s.name.clone()).collect() }
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
        Shape::Scalar => None,
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
}

impl<'a> FnCompiler<'a> {
    fn new(f: &'a TFunction, globals: Option<&'a [Slot]>, strings: &'a mut Strings) -> FnCompiler<'a> {
        let n = f.locals.len() as Reg;
        FnCompiler { f, globals, strings, code: Vec::new(), positions: Vec::new(), n_locals: n, next: n, max: n }
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
            Op::Jump(t) | Op::JumpIfFalse(_, t) | Op::JumpIfTrue(_, t) | Op::JumpUnlessI(_, _, _, t) => *t = target,
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
                let x = self.operand(a);
                let y = self.operand(b);
                return self.emit(Op::JumpUnlessI(cmp, x, y, 0), c.pos);
            }
        }
        let r = self.operand(c);
        self.emit(Op::JumpIfFalse(r, 0), c.pos)
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
            TExprKind::Length(x) => {
                let r = self.operand(x);
                self.emit(Op::Length(d, r), pos);
            }
            TExprKind::Builtin(b, args) => {
                let first = self.args(args);
                self.emit(Op::Builtin { id: b.id(), first, argc: args.len() as u16, dst: d }, pos);
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
                self.block(body);
                self.emit(Op::Jump(top), c.pos);
                let end = self.here();
                self.patch(exit, end);
            }
            TStmt::Repeat(body, c) => {
                let top = self.here();
                self.block(body);
                let r = self.operand(c);
                self.emit(Op::JumpIfTrue(r, top), c.pos);
            }
            TStmt::For { var, from, to, step, body, pos } => self.for_loop(*var, from, to, step.as_ref(), body, *pos),
            TStmt::Call(f, args, pos) => {
                let first = self.args(args);
                let dst = self.temp();
                self.emit(Op::Call { func: *f as u32, first, argc: args.len() as u16, dst }, *pos);
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
        self.block(body);
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
        assert!(code.iter().any(|op| matches!(op, crate::bytecode::Op::JumpUnlessI(..))), "{:?}", code);
        assert!(!code.iter().any(|op| matches!(op, crate::bytecode::Op::CmpI(..))), "{:?}", code);
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
        assert!(run_src("variables:\n    n as int\ninput(n)\n", "9,99\n").contains("expected an integer, read \"9,99\""));
    }
}
