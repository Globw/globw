//! The instruction set of the VM. Instructions name registers of the
//! current function; jump targets are instruction indices.

use std::rc::Rc;

use crate::errors::Pos;
use crate::tir::BaseType;

pub type Reg = u16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    LoadInt(Reg, i64),
    LoadReal(Reg, f64),
    LoadBool(Reg, bool),
    /// load `Module::strings[index]`
    LoadStr(Reg, u32),
    /// dst, src
    Move(Reg, Reg),
    // dst, a, b
    AddI(Reg, Reg, Reg),
    SubI(Reg, Reg, Reg),
    MulI(Reg, Reg, Reg),
    DivI(Reg, Reg, Reg),
    ModI(Reg, Reg, Reg),
    AddR(Reg, Reg, Reg),
    SubR(Reg, Reg, Reg),
    MulR(Reg, Reg, Reg),
    DivR(Reg, Reg, Reg),
    Concat(Reg, Reg, Reg),
    // dst, src
    NegI(Reg, Reg),
    NegR(Reg, Reg),
    Not(Reg, Reg),
    ToReal(Reg, Reg),
    ToStr(Reg, Reg),
    // comparison, dst, a, b
    CmpI(Cmp, Reg, Reg, Reg),
    CmpR(Cmp, Reg, Reg, Reg),
    CmpS(Cmp, Reg, Reg, Reg),
    CmpB(Cmp, Reg, Reg, Reg),
    Jump(u32),
    JumpIfFalse(Reg, u32),
    JumpIfTrue(Reg, u32),
    /// jumps to the target when the integer comparison a <cmp> b is false
    JumpUnlessI(Cmp, Reg, Reg, u32),
    /// jumps to the target when `a <cmp> imm` is false
    JumpUnlessIImm(Cmp, Reg, i64, u32),
    /// dst = a <op> imm, for integer arithmetic with a constant operand
    AddIImm(Reg, Reg, i64),
    SubIImm(Reg, Reg, i64),
    MulIImm(Reg, Reg, i64),
    DivIImm(Reg, Reg, i64),
    ModIImm(Reg, Reg, i64),
    /// dst = (a <cmp> imm)
    CmpIImm(Cmp, Reg, Reg, i64),
    /// fails if step is 0; jumps to `exit` if the loop must not run at all
    ForInit { var: Reg, end: Reg, step: Reg, exit: u32 },
    /// var += step; jumps back to `body` while the loop continues
    ForStep { var: Reg, end: Reg, step: Reg, body: u32 },
    /// print without newline
    Print(Reg),
    PrintLn,
    GetGlobal(Reg, u16),
    SetGlobal(u16, Reg),
    /// new array of n empty elements
    NewArray(Reg, u32),
    /// new rows × cols matrix
    NewMatrix(Reg, u32, u32),
    NewList(Reg),
    NewDict(Reg),
    /// dst, class id, number of fields
    NewObject(Reg, u32, u32),
    /// dst, object, field slot
    GetField(Reg, Reg, u16),
    /// object, field slot, src
    SetField(Reg, u16, Reg),
    /// dst, array/list, index (1-based)
    GetElem(Reg, Reg, Reg),
    /// array/list, index, src
    SetElem(Reg, Reg, Reg),
    /// dst, dictionary, key
    GetKey(Reg, Reg, Reg),
    /// dictionary, key, src
    SetKey(Reg, Reg, Reg),
    /// dst, matrix, row, column
    GetElem2(Reg, Reg, Reg, Reg),
    /// matrix, row, column, src
    SetElem2(Reg, Reg, Reg, Reg),
    /// dst, string, index
    CharAt(Reg, Reg, Reg),
    /// dst, array/list/string
    Length(Reg, Reg),
    /// list, src
    Append(Reg, Reg),
    /// list, index
    Remove(Reg, Reg),
    /// copies `argc` registers from `first` into the callee's registers 0..argc
    Call { func: u32, first: Reg, argc: u16, dst: Reg },
    Ret(Reg),
    Read(Reg, BaseType),
    /// virtual method call: self is the register `first`; its runtime class
    /// and the dispatch `slot` pick the function via the module's vtable
    CallMethod { slot: u32, first: Reg, argc: u16, dst: Reg },
    /// dst = (object at `obj` is of class `class` or a subclass)
    IsClass(Reg, Reg, u32),
    /// load a function value (its compiled function index) into a register
    LoadFunc(Reg, u32),
    /// call the function whose index is the value in register `func`
    CallValue { func: Reg, first: Reg, argc: u16, dst: Reg },
    /// calls built-in `id` (see `builtins::Builtin::from_id`) with `argc`
    /// registers from `first`; a function's result goes to `dst`
    Builtin { id: u16, first: Reg, argc: u16, dst: Reg },
    RetNone,
    /// install an error handler: on a run-time error jump to `catch_ip` with
    /// the message stored in register `err`
    PushHandler(u32, Reg),
    /// remove the most recent error handler
    PopHandler,
    /// raise a run-time error with the message in the register
    Fail(Reg),
    /// if global `flag` is not true: sets it and calls function `func` with no
    /// arguments (a module's start-up code, run at most once)
    Ensure { flag: u16, func: u32 },
}

impl Op {
    /// The variant name, for the opcode profiler.
    pub fn name(&self) -> &'static str {
        match self {
            Op::LoadInt(..) => "LoadInt",
            Op::LoadReal(..) => "LoadReal",
            Op::LoadBool(..) => "LoadBool",
            Op::LoadStr(..) => "LoadStr",
            Op::Move(..) => "Move",
            Op::AddI(..) => "AddI",
            Op::SubI(..) => "SubI",
            Op::MulI(..) => "MulI",
            Op::DivI(..) => "DivI",
            Op::ModI(..) => "ModI",
            Op::AddR(..) => "AddR",
            Op::SubR(..) => "SubR",
            Op::MulR(..) => "MulR",
            Op::DivR(..) => "DivR",
            Op::Concat(..) => "Concat",
            Op::NegI(..) => "NegI",
            Op::NegR(..) => "NegR",
            Op::Not(..) => "Not",
            Op::ToReal(..) => "ToReal",
            Op::ToStr(..) => "ToStr",
            Op::CmpI(..) => "CmpI",
            Op::CmpR(..) => "CmpR",
            Op::CmpS(..) => "CmpS",
            Op::CmpB(..) => "CmpB",
            Op::Jump(..) => "Jump",
            Op::JumpIfFalse(..) => "JumpIfFalse",
            Op::JumpIfTrue(..) => "JumpIfTrue",
            Op::JumpUnlessI(..) => "JumpUnlessI",
            Op::JumpUnlessIImm(..) => "JumpUnlessIImm",
            Op::AddIImm(..) => "AddIImm",
            Op::SubIImm(..) => "SubIImm",
            Op::MulIImm(..) => "MulIImm",
            Op::DivIImm(..) => "DivIImm",
            Op::ModIImm(..) => "ModIImm",
            Op::CmpIImm(..) => "CmpIImm",
            Op::ForInit { .. } => "ForInit",
            Op::ForStep { .. } => "ForStep",
            Op::Print(..) => "Print",
            Op::PrintLn => "PrintLn",
            Op::GetGlobal(..) => "GetGlobal",
            Op::SetGlobal(..) => "SetGlobal",
            Op::NewArray(..) => "NewArray",
            Op::NewMatrix(..) => "NewMatrix",
            Op::NewList(..) => "NewList",
            Op::NewDict(..) => "NewDict",
            Op::NewObject(..) => "NewObject",
            Op::GetField(..) => "GetField",
            Op::SetField(..) => "SetField",
            Op::GetElem(..) => "GetElem",
            Op::GetKey(..) => "GetKey",
            Op::SetKey(..) => "SetKey",
            Op::SetElem(..) => "SetElem",
            Op::GetElem2(..) => "GetElem2",
            Op::SetElem2(..) => "SetElem2",
            Op::CharAt(..) => "CharAt",
            Op::Length(..) => "Length",
            Op::Append(..) => "Append",
            Op::Remove(..) => "Remove",
            Op::Call { .. } => "Call",
            Op::CallMethod { .. } => "CallMethod",
            Op::IsClass(..) => "IsClass",
            Op::LoadFunc(..) => "LoadFunc",
            Op::CallValue { .. } => "CallValue",
            Op::Ret(..) => "Ret",
            Op::Read(..) => "Read",
            Op::Builtin { .. } => "Builtin",
            Op::RetNone => "RetNone",
            Op::PushHandler(..) => "PushHandler",
            Op::PopHandler => "PopHandler",
            Op::Fail(..) => "Fail",
            Op::Ensure { .. } => "Ensure",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub n_params: u16,
    pub n_regs: u16,
    pub code: Vec<Op>,
    /// source position of each instruction, for run-time errors
    pub positions: Vec<Pos>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Module {
    /// index 0 is the main program
    pub functions: Vec<Function>,
    pub strings: Vec<Rc<str>>,
    /// global variable names, for run-time error messages
    pub globals: Vec<String>,
    /// per class, how each field of a fresh object is initialized (see
    /// `tir::class_field_kinds`); precomputed so `new` allocates no extra memory
    pub class_fields: Vec<Vec<u8>>,
    /// per class, the dispatch row: method slot -> function index, or -1
    pub vtable: Vec<Vec<i32>>,
    /// per class, the parent class id, or -1
    pub class_parents: Vec<i32>,
}
