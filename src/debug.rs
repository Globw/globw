//! The interface between the VM and a debugger.
//!
//! When a program runs with a [`DebugHook`], the VM calls [`DebugHook::on_line`]
//! each time execution reaches a new source line (or the same line at another
//! call depth). The hook decides whether to stop there; while it is stopped it
//! can read the program's state through [`DebugState`] — the call stack, and the
//! variables of each frame and the globals — then returns to resume or end the
//! program. The hook knows nothing of a protocol: the `gw --dap` adapter for
//! editors is built on top of it.

use crate::bytecode::Module;
use crate::vm::{display, repr, Value};

/// What the program does after the hook returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    /// go on running
    Continue,
    /// stop the program now (it ends without an error)
    Terminate,
}

/// Called by the VM at every new source line of a debugged program.
pub trait DebugHook {
    fn on_line(&mut self, state: &DebugState) -> DebugAction;

    /// Called when a run-time error is not caught by any `try`, before the
    /// program ends with it: the state is still the one at the failing line.
    fn on_error(&mut self, _state: &DebugState, _message: &str) {}
}

/// One frame of the call stack, innermost first.
#[derive(Debug, Clone)]
pub struct DebugFrame {
    /// the function name, or "main program"
    pub name: String,
    /// the index of the source file (as in `compile_program`'s file list)
    pub file: u16,
    pub line: u32,
    pub col: u32,
    /// the frame's variables that have a value
    pub locals: Vec<DebugVar>,
}

/// A variable (or an element of a collection) with its value as text and, for
/// lists, dictionaries, tuples and objects, its parts.
#[derive(Debug, Clone)]
pub struct DebugVar {
    pub name: String,
    pub value: String,
    /// "int", "list", "Point"...
    pub kind: String,
    pub children: Vec<DebugVar>,
}

/// The program's state where the debugger is called, for reading only.
pub struct DebugState<'a> {
    pub(crate) module: &'a Module,
    pub(crate) regs: &'a [Value],
    pub(crate) globals: &'a [Value],
    /// (function, ip, base) of every active frame, innermost first; for the
    /// innermost `ip` points just past the current instruction, for the others
    /// just past their call
    pub(crate) frames: Vec<(usize, usize, usize)>,
    /// how many calls deep the current line is (0 in the main program)
    pub depth: usize,
    /// true when the line was reached by returning from a call
    pub returned: bool,
}

/// How many elements of a collection are shown, and how deep parts nest.
const MAX_ITEMS: usize = 200;
const MAX_DEPTH: usize = 4;

impl DebugState<'_> {
    /// The file and line being executed.
    pub fn file(&self) -> u16 {
        self.position(0).0
    }

    pub fn line(&self) -> u32 {
        self.position(0).1
    }

    fn position(&self, frame: usize) -> (u16, u32, u32) {
        let (func, ip, _) = self.frames[frame];
        let p = self.module.functions[func].positions[ip.saturating_sub(1)];
        (p.file, p.line, p.col)
    }

    /// The call stack, innermost first, with each frame's variables.
    pub fn frames(&self) -> Vec<DebugFrame> {
        (0..self.frames.len())
            .map(|k| {
                let (func, _, base) = self.frames[k];
                let f = &self.module.functions[func];
                let (file, line, col) = self.position(k);
                let locals = f
                    .locals
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| !name.is_empty() && !name.starts_with("__") && !name.contains('#'))
                    .filter_map(|(i, name)| {
                        let v = self.regs.get(base + i)?;
                        (!matches!(v, Value::Null)).then(|| var(name.clone(), v, 0))
                    })
                    .collect();
                let name = if func == 0 || f.name.is_empty() { "main program".to_string() } else { f.name.replace('#', ".") };
                DebugFrame { name, file, line, col, locals }
            })
            .collect()
    }

    /// The global variables that have a value.
    pub fn globals(&self) -> Vec<DebugVar> {
        self.module
            .globals
            .iter()
            .zip(self.globals)
            .filter(|(name, v)| !name.contains("__") && !matches!(v, Value::Null))
            .map(|(name, v)| var(name.clone(), v, 0))
            .collect()
    }
}

fn var(name: String, v: &Value, depth: usize) -> DebugVar {
    let short = |s: String| if s.chars().count() > 300 { format!("{}…", s.chars().take(299).collect::<String>()) } else { s };
    let (kind, children): (String, Vec<DebugVar>) = match v {
        Value::Int(_) => ("int".into(), vec![]),
        Value::Real(_) => ("float".into(), vec![]),
        Value::Bool(_) => ("bool".into(), vec![]),
        Value::Str(_) => ("string".into(), vec![]),
        Value::Packed(p) => {
            let items = p.borrow().values();
            let kids = if depth < MAX_DEPTH {
                items.iter().take(MAX_ITEMS).enumerate().map(|(i, x)| var(format!("[{}]", i + 1), x, depth + 1)).collect()
            } else {
                vec![]
            };
            (format!("array ({})", items.len()), kids)
        }
        Value::Seq(q) => {
            let items = q.borrow();
            let kids = if depth < MAX_DEPTH {
                items.iter().take(MAX_ITEMS).enumerate().map(|(i, x)| var(format!("[{}]", i + 1), x, depth + 1)).collect()
            } else {
                vec![]
            };
            (format!("list ({})", items.len()), kids)
        }
        Value::Dict(d) => {
            let d = d.borrow();
            let kids = if depth < MAX_DEPTH {
                d.keys()
                    .zip(d.values())
                    .take(MAX_ITEMS)
                    .map(|(k, x)| var(format!("[{}]", repr(&k.value())), x, depth + 1))
                    .collect()
            } else {
                vec![]
            };
            (format!("dict ({})", d.len()), kids)
        }
        Value::Set(d) => ("set".to_string() + &format!(" ({})", d.borrow().len()), vec![]),
        Value::Object(o) => {
            let o = o.borrow();
            let class = crate::tir::class_name(o.class);
            let names = crate::tir::class_field_names(o.class);
            let kids = if depth < MAX_DEPTH {
                o.fields
                    .iter()
                    .enumerate()
                    .map(|(i, x)| var(names.get(i).cloned().unwrap_or_else(|| format!("field {}", i + 1)), x, depth + 1))
                    .collect()
            } else {
                vec![]
            };
            (class, kids)
        }
        Value::Matrix(_) => ("matrix".into(), vec![]),
        Value::Floats(_, shape) => (format!("nungw {:?}", shape.as_slice()), vec![]),
        Value::Func(_) => ("function".into(), vec![]),
        _ => (String::new(), vec![]),
    };
    let value = match v {
        Value::Str(_) => short(repr(v)),
        other => short(display(other)),
    };
    DebugVar { name, value, kind, children }
}
