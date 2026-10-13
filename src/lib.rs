//! Easier: an interpreter for school-style pseudocode.

pub mod errors;
pub mod lexer;
pub mod ast;
pub mod parser;
pub mod tir;
pub mod checker;
pub mod bytecode;
pub mod vm;
