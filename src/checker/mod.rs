//! Semantic analysis: name resolution, type inference, specialization of
//! functions per argument types, definite assignment and return checks.
//! Produces the typed program consumed by the compiler.

mod calls;
mod exprs;
mod stmts;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use crate::ast::{Decl, Program, Shape, Stmt, StmtKind};
use crate::errors::{Error, Pos};
use crate::tir::*;

pub const BUILTINS: [&str; 5] = ["input", "output", "LENGTH", "APPEND", "REMOVE"];

/// A variable as the checker sees it. For arrays, matrices and lists `ty`
/// is the element type.
#[derive(Debug, Clone)]
pub(crate) struct Var {
    pub(crate) name: String,
    pub(crate) shape: Shape,
    pub(crate) ty: Option<BaseType>,
    /// where the type was first fixed, for conflict messages
    pub(crate) origin: Option<Pos>,
    pub(crate) conflict: Option<Error>,
    pub(crate) is_param: bool,
}

impl Var {
    pub(crate) fn declared(d: &Decl) -> Var {
        Var { name: d.name.clone(), shape: d.shape.clone(), ty: None, origin: None, conflict: None, is_param: false }
    }

    pub(crate) fn full_type(&self) -> Type {
        match self.shape {
            Shape::Scalar => self.ty.map_or(Type::Unknown, Type::Base),
            Shape::Array(_) => Type::Array(self.ty),
            Shape::Matrix(..) => Type::Matrix(self.ty),
            Shape::List => Type::List(self.ty),
        }
    }
}

/// One specialization: the main program (`sub == None`) or a subprogram
/// checked for one combination of argument types.
pub(crate) struct Instance {
    pub(crate) sub: Option<usize>,
    pub(crate) locals: Vec<Var>,
    pub(crate) ret: Option<BaseType>,
    pub(crate) ret_origin: Option<Pos>,
    pub(crate) final_index: Option<usize>,
}

pub(crate) struct Checker<'p> {
    pub(crate) prog: &'p Program,
    pub(crate) globals: Vec<Var>,
    pub(crate) instances: Vec<Instance>,
    pub(crate) by_key: HashMap<(usize, Vec<Type>), usize>,
    pub(crate) sub_by_name: HashMap<String, usize>,
    /// set whenever a type, a return type or the set of instances grows
    pub(crate) changed: bool,
    /// true only in the last round: errors are reported and IR is kept
    pub(crate) is_final: bool,
    /// true while checking a never-called subprogram: only structural errors count
    pub(crate) names_only: bool,
    /// true only in a guessing round: operator hints may give types to unknown variables
    pub(crate) hints: bool,
    pub(crate) errors: Vec<Error>,
    pub(crate) current: usize,
    pub(crate) queue: Vec<usize>,
    pub(crate) visited: HashSet<usize>,
    /// definite assignment state of the current instance's locals
    pub(crate) assigned: Vec<bool>,
    pub(crate) final_queue: Vec<usize>,
}

pub fn check(prog: &Program) -> Result<TProgram, Vec<Error>> {
    let mut c = Checker::new(prog);
    c.check_declarations();
    if !c.errors.is_empty() {
        return Err(c.errors);
    }
    // Real types (literals, assignments, returns) settle first. Only then do
    // guesses from how unknown variables are used fill the gaps, one round at
    // a time, and only then does anything still unknown become a string.
    let mut rounds = 0;
    loop {
        loop {
            c.changed = false;
            c.round();
            rounds += 1;
            assert!(rounds < 10_000, "internal error: type inference does not converge");
            if !c.changed {
                break;
            }
        }
        c.hints = true;
        c.changed = false;
        c.round();
        c.hints = false;
        if c.changed {
            continue;
        }
        if !c.default_unknowns() {
            break;
        }
    }
    c.is_final = true;
    let program = c.final_round();
    if c.errors.is_empty() {
        Ok(program)
    } else {
        Err(c.errors)
    }
}

impl<'p> Checker<'p> {
    fn new(prog: &'p Program) -> Checker<'p> {
        let main = Instance {
            sub: None,
            locals: prog.locals.iter().map(Var::declared).collect(),
            ret: None,
            ret_origin: None,
            final_index: None,
        };
        Checker {
            prog,
            globals: prog.globals.iter().map(Var::declared).collect(),
            instances: vec![main],
            by_key: HashMap::new(),
            sub_by_name: prog.subprograms.iter().enumerate().map(|(i, s)| (s.name.clone(), i)).collect(),
            changed: false,
            is_final: false,
            names_only: false,
            hints: false,
            errors: Vec::new(),
            current: 0,
            queue: Vec::new(),
            visited: HashSet::new(),
            assigned: Vec::new(),
            final_queue: Vec::new(),
        }
    }

    /// Records an error that depends on types. Skipped in never-called subprograms.
    pub(crate) fn report(&mut self, e: Error) {
        if !self.names_only {
            self.report_structural(e);
        }
    }

    /// Records an error that does not depend on types. Only the final round
    /// reports; duplicates are dropped and at most 20 errors are kept.
    pub(crate) fn report_structural(&mut self, e: Error) {
        if self.is_final && self.errors.len() < 20 && !self.errors.contains(&e) {
            self.errors.push(e);
        }
    }

    fn check_declarations(&mut self) {
        let prog = self.prog;
        let mut functions = HashSet::new();
        for s in &prog.subprograms {
            if BUILTINS.contains(&s.name.as_str()) {
                self.errors.push(Error::new(s.pos, format!("\"{}\" is a built-in name: choose another one", s.name)));
            } else if !functions.insert(s.name.clone()) {
                self.errors.push(Error::new(s.pos, format!("function \"{}\" is defined twice", s.name)));
            }
        }
        let globals: Vec<(&str, Pos)> = prog.globals.iter().map(|d| (d.name.as_str(), d.pos)).collect();
        self.check_names(&globals, &[], &functions);
        let main: Vec<(&str, Pos)> = prog.locals.iter().map(|d| (d.name.as_str(), d.pos)).collect();
        self.check_names(&main, &globals, &functions);
        for s in &prog.subprograms {
            let mut names: Vec<(&str, Pos)> = s.params.iter().map(|(n, p)| (n.as_str(), *p)).collect();
            names.extend(s.locals.iter().map(|d| (d.name.as_str(), d.pos)));
            self.check_names(&names, &[], &functions);
        }
    }

    fn check_names(&mut self, names: &[(&str, Pos)], globals: &[(&str, Pos)], functions: &HashSet<String>) {
        let mut seen = HashSet::new();
        for &(name, pos) in names {
            let message = if !seen.insert(name) {
                format!("\"{}\" is declared twice", name)
            } else if BUILTINS.contains(&name) {
                format!("\"{}\" is a built-in name: choose another one", name)
            } else if functions.contains(name) {
                format!("\"{}\" is already the name of a function", name)
            } else if globals.iter().any(|(g, _)| *g == name) {
                format!("\"{}\" is already declared as GLOBAL", name)
            } else {
                continue;
            };
            self.errors.push(Error::new(pos, message));
        }
    }

    /// One inference round: visits the main program and every instance it reaches.
    fn round(&mut self) {
        self.visited.clear();
        self.visited.insert(0);
        self.queue = vec![0];
        while let Some(i) = self.queue.pop() {
            self.visit(i);
        }
    }

    /// Notes that instance `i` is called. In inference rounds it gets queued
    /// for a visit; in the final round it gets its index in the output program.
    pub(crate) fn reach(&mut self, i: usize) -> usize {
        if self.names_only {
            return 0;
        }
        if self.is_final {
            if let Some(k) = self.instances[i].final_index {
                return k;
            }
            let k = self.final_queue.len();
            self.final_queue.push(i);
            self.instances[i].final_index = Some(k);
            return k;
        }
        if self.visited.insert(i) {
            self.queue.push(i);
        }
        0
    }

    pub(crate) fn visit(&mut self, i: usize) -> Vec<TStmt> {
        let prog = self.prog;
        self.current = i;
        self.assigned = self.instances[i].locals.iter().map(|v| v.is_param || v.shape != Shape::Scalar).collect();
        let body = match self.instances[i].sub {
            None => &prog.body,
            Some(s) => &prog.subprograms[s].body,
        };
        let out = self.block(body);
        if let Some(s) = self.instances[i].sub {
            let sub = &prog.subprograms[s];
            if sub.is_function && !always_returns(&sub.body) {
                self.report_structural(
                    Error::new(sub.pos, format!("function \"{}\" can reach END FUNCTION without RETURN", sub.name))
                        .with_hint("add RETURN value at the end"),
                );
            }
        }
        out
    }

    /// Gives the string type to everything whose type is still unknown (for
    /// example a variable only ever read with input). Returns true if anything changed.
    fn default_unknowns(&mut self) -> bool {
        let mut changed = false;
        let visited: Vec<usize> = self.visited.iter().copied().collect();
        for i in visited {
            for v in &mut self.instances[i].locals {
                if v.ty.is_none() {
                    v.ty = Some(BaseType::Str);
                    changed = true;
                }
            }
        }
        for v in &mut self.globals {
            if v.ty.is_none() {
                v.ty = Some(BaseType::Str);
                changed = true;
            }
        }
        changed
    }

    fn final_round(&mut self) -> TProgram {
        self.reach(0);
        let mut functions = Vec::new();
        let mut k = 0;
        while k < self.final_queue.len() {
            let i = self.final_queue[k];
            let body = self.visit(i);
            functions.push(self.function(i, body));
            k += 1;
        }
        self.check_uncalled();
        let mut conflicts: Vec<Error> = Vec::new();
        for &i in &self.final_queue {
            conflicts.extend(self.instances[i].locals.iter().filter_map(|v| v.conflict.clone()));
        }
        conflicts.extend(self.globals.iter().filter_map(|v| v.conflict.clone()));
        for e in conflicts {
            self.report(e);
        }
        self.errors.sort_by_key(|e| (e.pos.line, e.pos.col));
        TProgram { globals: self.globals.iter().map(slot).collect(), functions }
    }

    /// Visits every subprogram that the final round never reached, so that
    /// names are checked even in code that is not called yet.
    fn check_uncalled(&mut self) {
        let prog = self.prog;
        for s in 0..prog.subprograms.len() {
            let reached = self.instances.iter().any(|i| i.sub == Some(s) && i.final_index.is_some());
            if reached {
                continue;
            }
            let key = vec![Type::Unknown; prog.subprograms[s].params.len()];
            let inst = self.instance(s, key);
            self.names_only = true;
            self.visit(inst);
            self.names_only = false;
        }
    }

    fn function(&self, i: usize, body: Vec<TStmt>) -> TFunction {
        let inst = &self.instances[i];
        let (name, n_params, is_function) = match inst.sub {
            None => ("main".to_string(), 0, false),
            Some(s) => {
                let sub = &self.prog.subprograms[s];
                (sub.name.clone(), sub.params.len(), sub.is_function)
            }
        };
        TFunction {
            name,
            n_params,
            locals: inst.locals.iter().map(slot).collect(),
            ret: if is_function { inst.ret } else { None },
            body,
        }
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<Place> {
        if let Some(i) = self.instances[self.current].locals.iter().position(|v| v.name == name) {
            return Some(Place::Local(i as u16));
        }
        self.globals.iter().position(|v| v.name == name).map(|i| Place::Global(i as u16))
    }

    pub(crate) fn var(&self, p: Place) -> &Var {
        match p {
            Place::Local(i) => &self.instances[self.current].locals[i as usize],
            Place::Global(i) => &self.globals[i as usize],
        }
    }

    fn var_mut(&mut self, p: Place) -> &mut Var {
        match p {
            Place::Local(i) => &mut self.instances[self.current].locals[i as usize],
            Place::Global(i) => &mut self.globals[i as usize],
        }
    }

    /// Joins `t` into the type of a variable (or of its elements), widening
    /// integer to real. An incompatible type is recorded as a conflict.
    pub(crate) fn widen(&mut self, p: Place, t: BaseType, pos: Pos) {
        let v = self.var_mut(p);
        let changed = match v.ty {
            None => {
                v.ty = Some(t);
                v.origin = Some(pos);
                true
            }
            Some(old) => match join(old, t) {
                Some(new) => {
                    v.ty = Some(new);
                    new != old
                }
                None => {
                    if v.conflict.is_none() {
                        v.conflict = Some(conflict_error(v, old, t, pos));
                    }
                    false
                }
            },
        };
        if changed {
            self.changed = true;
        }
    }

    /// A guess: if `e` is a variable (or an element) whose type is still
    /// unknown, gives it type `t`. Guesses only apply in guessing rounds and
    /// never change a type that is already known.
    pub(crate) fn hint(&mut self, e: &TExpr, t: BaseType) {
        if !self.hints || e.ty != Type::Unknown {
            return;
        }
        match &e.kind {
            TExprKind::Load(p) | TExprKind::Elem(p, _) if self.var(*p).ty.is_none() => self.widen(*p, t, e.pos),
            _ => {}
        }
    }
}

fn conflict_error(v: &Var, old: BaseType, new: BaseType, pos: Pos) -> Error {
    let line = v.origin.map_or(String::new(), |p| format!(" (line {})", p.line));
    let message = if v.shape == Shape::Scalar {
        format!("\"{}\" is {}{}, but here it gets {}", v.name, old.name(), line, new.name())
    } else {
        format!("the elements of \"{}\" are {}{}, but here they get {}", v.name, old.plural(), line, new.name())
    };
    Error::new(pos, message)
}

fn slot(v: &Var) -> Slot {
    Slot { name: v.name.clone(), ty: v.full_type(), shape: v.shape.clone(), is_param: v.is_param }
}

/// True if every path through `body` ends in RETURN.
pub(crate) fn always_returns(body: &[Stmt]) -> bool {
    body.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::If { branches, else_body: Some(e) } => {
            branches.iter().all(|(_, b)| always_returns(b)) && always_returns(e)
        }
        StmtKind::Repeat(b, _) => always_returns(b),
        _ => false,
    })
}
