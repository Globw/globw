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
    /// dst, collection, index (1-based)
    GetElem(Reg, Reg, Reg),
    /// collection, index, src
    SetElem(Reg, Reg, Reg),
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
    RetNone,
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
}
