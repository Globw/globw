//! Modules: `import`, `from ... import` and `lazy import`.
//!
//! Every imported file is parsed and merged with the main program into one
//! `Program` before checking. The names of a module get its name as a prefix
//! (`shapes.area`), and its start-up code becomes the procedure
//! `shapes.__init__`, which a `LoadModule` statement runs at most once:
//! - a normal import loads the module where the import is written;
//! - every statement that uses a module from another file loads it first,
//!   so a `lazy import` loads it just before the first statement that uses it,
//!   and a module that is never used never runs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::*;
use crate::builtins::is_builtin_module;
use crate::checker::is_builtin_name;

/// Alias target for a built-in module (one provided by the interpreter).
const BUILTIN_MODULE: usize = usize::MAX;
use crate::errors::{Error, Pos, SourceFile};
use crate::tir::{BaseType, Type};

const INIT: &str = "__init__";
const LOADED: &str = "__loaded__";

struct Module {
    /// "" for the main program
    name: String,
    dir: PathBuf,
    program: Program,
}

/// What every file defines and imports, for resolving names.
struct Tables {
    names: Vec<String>,
    globals: Vec<HashSet<String>>,
    subs: Vec<HashSet<String>>,
    /// module (or alias) name → module, for `import`
    aliases: Vec<HashMap<String, usize>>,
    /// name → (module, its name there), for `from ... import`
    from: Vec<HashMap<String, (usize, String)>>,
}

impl Tables {
    /// The name of `name` of module `m` in the merged program.
    fn qualified(&self, m: usize, name: &str) -> String {
        if m == 0 {
            name.to_string()
        } else {
            format!("{}.{}", self.names[m], name)
        }
    }

    fn defines(&self, m: usize, name: &str) -> bool {
        self.globals[m].contains(name) || self.subs[m].contains(name)
    }

    fn load(&self, m: usize, pos: Pos) -> Stmt {
        Stmt { kind: StmtKind::LoadModule { init: self.qualified(m, INIT), flag: self.qualified(m, LOADED) }, pos }
    }
}

/// Reads the main program and every module it imports, and merges them.
/// The files come back in any case, so that errors can show their lines.
pub fn load(main: SourceFile, dir: &Path) -> (Vec<SourceFile>, Result<Program, Vec<Error>>) {
    let main_stem = main.name.strip_suffix(".gw").unwrap_or(&main.name).to_string();
    let mut files = vec![main];
    let program = match parse_file(&files[0].text, 0) {
        Ok(p) => p,
        Err(e) => return (files, Err(vec![e])),
    };
    let mut modules = vec![Module { name: String::new(), dir: dir.to_path_buf(), program }];
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut k = 0;
    while k < modules.len() {
        let imports = modules[k].program.imports.clone();
        for imp in &imports {
            if by_name.contains_key(&imp.module) {
                continue;
            }
            if is_builtin_module(&imp.module) {
                continue; // provided by the interpreter, no file to load
            }
            if imp.module == modules[k].name {
                return (files, Err(vec![Error::new(imp.pos, "a module cannot import itself")]));
            }
            let file_name = format!("{}.gw", imp.module);
            // search: next to the importing file, the main program's folder, a
            // "libs" folder there, then every directory on GLOBW_PATH (a minimal
            // package path).
            let mut candidates = vec![
                modules[k].dir.join(&file_name),
                dir.join(&file_name),
                dir.join("libs").join(&file_name),
            ];
            if let Some(path_var) = std::env::var_os("GLOBW_PATH") {
                for d in std::env::split_paths(&path_var) {
                    candidates.push(d.join(&file_name));
                }
            }
            let Some(path) = candidates.iter().find(|p| p.is_file()).cloned() else {
                let e = Error::new(imp.pos, format!("module \"{}\" not found", imp.module)).with_hint(format!(
                    "put {} next to the program, in a \"libs\" folder there, or on GLOBW_PATH",
                    file_name
                ));
                return (files, Err(vec![e]));
            };
            if imp.module == main_stem && path.parent() == Some(dir) {
                let e = Error::new(imp.pos, "a module cannot import the main program")
                    .with_hint("move what both files need into a third module");
                return (files, Err(vec![e]));
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                    let e = Error::new(imp.pos, format!("cannot read {}: it is not saved as UTF-8", file_name))
                        .with_hint("in VS Code click the encoding in the status bar and choose \"Save with Encoding\", UTF-8");
                    return (files, Err(vec![e]));
                }
                Err(e) => return (files, Err(vec![Error::new(imp.pos, format!("cannot read {}: {}", file_name, e))])),
            };
            let id = files.len();
            files.push(SourceFile { name: file_name, text });
            let program = match parse_file(&files[id].text, id as u16) {
                Ok(p) => p,
                Err(e) => return (files, Err(vec![e])),
            };
            let dir = path.parent().map_or_else(|| dir.to_path_buf(), Path::to_path_buf);
            by_name.insert(imp.module.clone(), modules.len());
            modules.push(Module { name: imp.module.clone(), dir, program });
        }
        k += 1;
    }
    let result = merge(modules, &by_name);
    (files, result)
}

fn parse_file(text: &str, file: u16) -> Result<Program, Error> {
    let in_file = |mut e: Error| {
        e.pos.file = file;
        e
    };
    let mut tokens = crate::lexer::tokenize(text).map_err(in_file)?;
    for t in &mut tokens {
        t.pos.file = file;
    }
    crate::parser::parse(tokens).map_err(in_file)
}

fn merge(mut modules: Vec<Module>, by_name: &HashMap<String, usize>) -> Result<Program, Vec<Error>> {
    let mut errors = Vec::new();
    let n = modules.len();
    let mut t = Tables {
        names: modules.iter().map(|m| m.name.clone()).collect(),
        globals: modules.iter().map(|m| m.program.globals.iter().map(|d| d.name.clone()).collect()).collect(),
        subs: modules.iter().map(|m| m.program.subprograms.iter().map(|s| s.name.clone()).collect()).collect(),
        aliases: vec![HashMap::new(); n],
        from: vec![HashMap::new(); n],
    };
    for (k, m) in modules.iter().enumerate().skip(1) {
        let decls = m.program.globals.iter().map(|d| (&d.name, d.pos));
        for (name, pos) in decls.chain(m.program.subprograms.iter().map(|s| (&s.name, s.pos))) {
            if is_builtin_name(name) {
                errors.push(Error::new(pos, format!("\"{}\" is a built-in name: choose another one", name)));
            } else if name.starts_with("__") {
                errors.push(Error::new(pos, "names starting with __ are reserved"));
            }
        }
        debug_assert_eq!(t.names[k], m.name);
    }
    for (k, m) in modules.iter().enumerate() {
        for imp in &m.program.imports {
            if is_builtin_module(&imp.module) {
                if imp.alias.is_some() || imp.names.is_some() {
                    errors.push(Error::new(
                        imp.pos,
                        format!("the built-in module \"{}\" is imported plainly: write import {}", imp.module, imp.module),
                    ));
                } else {
                    t.aliases[k].insert(imp.module.clone(), BUILTIN_MODULE);
                }
                continue;
            }
            let target = by_name[&imp.module];
            match &imp.names {
                None => {
                    let alias = imp.alias.clone().unwrap_or_else(|| imp.module.clone());
                    if t.defines(k, &alias) {
                        errors.push(Error::new(imp.pos, format!("\"{}\" is already the name of a function or variable of this file", alias)));
                    }
                    match t.aliases[k].insert(alias.clone(), target) {
                        Some(other) if other != target => {
                            errors.push(Error::new(imp.pos, format!("\"{}\" is the name of two different modules", alias)))
                        }
                        _ => {}
                    }
                }
                Some(names) => {
                    for item in names {
                        let local = item.alias.clone().unwrap_or_else(|| item.name.clone());
                        if !t.defines(target, &item.name) || item.name.starts_with("__") {
                            errors.push(Error::new(
                                item.pos,
                                format!("module \"{}\" has no function or variable called \"{}\"", imp.module, item.name),
                            ));
                        } else if t.defines(k, &local) {
                            errors.push(Error::new(item.pos, format!("\"{}\" is already defined in this file", local)));
                        } else {
                            t.from[k].insert(local, (target, item.name.clone()));
                        }
                    }
                }
            }
        }
    }
    for (k, m) in modules.iter().enumerate() {
        let p = &m.program;
        let locals = p.locals.iter().chain(p.subprograms.iter().flat_map(|s| s.params.iter().chain(&s.locals)));
        for d in locals {
            if t.aliases[k].contains_key(&d.name) {
                errors.push(Error::new(d.pos, format!("\"{}\" is the name of an imported module: choose another one", d.name)));
            } else if t.from[k].contains_key(&d.name) {
                errors.push(Error::new(d.pos, format!("\"{}\" is imported from a module: choose another name", d.name)));
            }
        }
    }
    for (k, m) in modules.iter().enumerate() {
        if k != 0 {
            if let Some(c) = m.program.classes.first() {
                errors.push(Error::new(
                    c.pos,
                    "classes can only be defined in the main program (not in an imported module) for now",
                ));
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut out = Program::default();
    for (k, m) in modules.iter_mut().enumerate() {
        let mut p = std::mem::take(&mut m.program);
        let classes = std::mem::take(&mut p.classes);
        let mut scope = Scope { k, t: &t, locals: HashSet::new(), uses: Vec::new(), errors: &mut errors };
        for mut c in classes {
            for meth in &mut c.methods {
                scope.locals = meth.params.iter().chain(&meth.locals).map(|d| d.name.clone()).collect();
                scope.locals.insert("self".to_string());
                let body = std::mem::take(&mut meth.body);
                meth.body = scope.block(body);
            }
            out.classes.push(c);
        }
        for d in p.globals {
            out.globals.push(Decl { name: t.qualified(k, &d.name), ..d });
        }
        for s in p.subprograms {
            scope.locals = s.params.iter().chain(&s.locals).map(|d| d.name.clone()).collect();
            let body = scope.block(s.body);
            out.subprograms.push(Subprogram { name: t.qualified(k, &s.name), body, ..s });
        }
        scope.locals = p.locals.iter().map(|d| d.name.clone()).collect();
        let mut body: Vec<Stmt> =
            p.imports
                .iter()
                .filter(|i| !i.lazy && !is_builtin_module(&i.module))
                .map(|i| t.load(by_name[&i.module], i.pos))
                .collect();
        body.extend(scope.block(p.body));
        if k == 0 {
            out.locals = p.locals;
            out.body = body;
        } else {
            let pos = Pos { line: 1, col: 0, len: 0, file: k as u16 };
            out.globals.push(Decl {
                name: t.qualified(k, LOADED),
                shape: Shape::Scalar,
                ty: BaseType::Bool,
                full: Type::Base(BaseType::Bool),
                default: None,
                pos,
            });
            out.subprograms.push(Subprogram {
                name: t.qualified(k, INIT),
                is_function: false,
                params: Vec::new(),
                ret: None,
                locals: p.locals,
                body,
                pos,
            });
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

/// Renames the names used in one file to their names in the merged program,
/// and loads the modules a statement uses just before it.
struct Scope<'a> {
    k: usize,
    t: &'a Tables,
    locals: HashSet<String>,
    /// modules of other files used by the current statement
    uses: Vec<usize>,
    errors: &'a mut Vec<Error>,
}

impl Scope<'_> {
    fn used(&mut self, m: usize) {
        if m != self.k && !self.uses.contains(&m) {
            self.uses.push(m);
        }
    }

    fn resolve(&mut self, name: &str, pos: Pos, call: bool) -> String {
        let (t, k) = (self.t, self.k);
        if let Some((prefix, rest)) = name.split_once('.') {
            if prefix == "self" || prefix == "parent" || self.locals.contains(prefix) {
                // field or method access on an object, or a super call: resolved by the checker
                return name.to_string();
            }
            if t.aliases[k].get(prefix) == Some(&BUILTIN_MODULE) {
                // e.g. text.matches -> the built-in function of that name
                return name.to_string();
            }
            let Some(&m) = t.aliases[k].get(prefix) else {
                if t.from[k].contains_key(prefix) || t.defines(k, prefix) || self.locals.contains(prefix) {
                    self.errors.push(Error::new(pos, format!("\"{}\" is not a module: \".\" cannot be used on it", prefix)));
                } else {
                    self.errors.push(
                        Error::new(pos, format!("\"{}\" is not an imported module", prefix))
                            .with_hint(format!("write import {} at the start of the file", prefix)),
                    );
                }
                return name.to_string();
            };
            if rest.contains('.') || rest.starts_with("__") || !t.defines(m, rest) {
                self.errors.push(Error::new(
                    pos,
                    format!("module \"{}\" has no function or variable called \"{}\"", t.names[m], rest),
                ));
                return name.to_string();
            }
            self.used(m);
            return t.qualified(m, rest);
        }
        if !call && self.locals.contains(name) {
            return name.to_string();
        }
        if t.defines(k, name) {
            return t.qualified(k, name);
        }
        if let Some((m, original)) = t.from[k].get(name) {
            let (m, original) = (*m, original.clone());
            self.used(m);
            return t.qualified(m, &original);
        }
        if k != 0 && t.defines(0, name) && !is_builtin_name(name) {
            // the merged program would find the main program's own name
            let message = if call {
                format!("function \"{}\" does not exist", name)
            } else {
                format!("variable \"{}\" is not declared", name)
            };
            self.errors.push(
                Error::new(pos, message)
                    .with_hint("a module cannot see the names of the main program: pass the values as parameters"),
            );
        }
        name.to_string()
    }

    fn block(&mut self, body: Vec<Stmt>) -> Vec<Stmt> {
        let mut out = Vec::with_capacity(body.len());
        for mut s in body {
            let outer = std::mem::take(&mut self.uses);
            self.stmt(&mut s);
            let uses = std::mem::replace(&mut self.uses, outer);
            out.extend(uses.into_iter().map(|m| self.t.load(m, s.pos)));
            out.push(s);
        }
        out
    }

    fn stmt(&mut self, s: &mut Stmt) {
        let pos = s.pos;
        match &mut s.kind {
            StmtKind::Assign(target, value) => {
                self.expr(value);
                target.name = self.resolve(&target.name, target.pos, false);
                target.indices.iter_mut().for_each(|e| self.expr(e));
                target.rest.iter_mut().flatten().for_each(|e| self.expr(e));
            }
            StmtKind::If { branches, else_body } => {
                for (cond, body) in branches.iter_mut() {
                    self.expr(cond);
                    *body = self.block(std::mem::take(body));
                }
                if let Some(body) = else_body {
                    *body = self.block(std::mem::take(body));
                }
            }
            StmtKind::While(cond, body) => {
                self.expr(cond);
                *body = self.block(std::mem::take(body));
            }
            StmtKind::For { var, var_pos, from, to, step, body } => {
                *var = self.resolve(var, *var_pos, false);
                self.expr(from);
                self.expr(to);
                if let Some(step) = step {
                    self.expr(step);
                }
                *body = self.block(std::mem::take(body));
            }
            StmtKind::Repeat(body, cond) => {
                *body = self.block(std::mem::take(body));
                self.expr(cond);
            }
            StmtKind::ForEach { var, var_pos, iter, body } => {
                *var = self.resolve(var, *var_pos, false);
                self.expr(iter);
                *body = self.block(std::mem::take(body));
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Nop => {}
            StmtKind::Call(name, args) => {
                *name = self.resolve(name, pos, true);
                args.iter_mut().for_each(|e| self.expr(e));
            }
            StmtKind::Return(value) => {
                if let Some(v) = value {
                    self.expr(v);
                }
            }
            StmtKind::Eval(e) => self.expr(e),
            StmtKind::SetMember(l, v) => {
                self.expr(l);
                self.expr(v);
            }
            StmtKind::Try { body, catch_name, catch_pos, catch, always } => {
                *body = self.block(std::mem::take(body));
                *catch_name = self.resolve(catch_name, *catch_pos, false);
                *catch = self.block(std::mem::take(catch));
                *always = self.block(std::mem::take(always));
            }
            StmtKind::Fail(e) => self.expr(e),
            StmtKind::Unpack(names, e) => {
                for (n, p) in names.iter_mut() {
                    *n = self.resolve(n, *p, false);
                }
                self.expr(e);
            }
            StmtKind::LoadModule { .. } => {}
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        let pos = e.pos;
        match &mut e.kind {
            ExprKind::Var(name) => *name = self.resolve(name, pos, false),
            ExprKind::Index(name, idx) => {
                *name = self.resolve(name, pos, false);
                idx.iter_mut().for_each(|e| self.expr(e));
            }
            ExprKind::Call(name, args) => {
                *name = self.resolve(name, pos, true);
                args.iter_mut().for_each(|e| self.expr(e));
            }
            ExprKind::Binary(_, a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Unary(_, a) => self.expr(a),
            ExprKind::Subscript(base, idx) => {
                self.expr(base);
                idx.iter_mut().for_each(|e| self.expr(e));
            }
            ExprKind::ListLit(items) => items.iter_mut().for_each(|e| self.expr(e)),
            ExprKind::Named(_, value) => self.expr(value),
            ExprKind::New(_, args) => args.iter_mut().for_each(|e| self.expr(e)),
            ExprKind::Field(b, _) => self.expr(b),
            ExprKind::MethodCall(b, _, args) => {
                self.expr(b);
                args.iter_mut().for_each(|e| self.expr(e));
            }
            ExprKind::IsType(b, _) => self.expr(b),
            ExprKind::FuncRef(n) => *n = self.resolve(n, crate::errors::Pos::default(), true),
            ExprKind::Int(_) | ExprKind::Real(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::NoneLit => {}
            ExprKind::TupleLit(items) => items.iter_mut().for_each(|e| self.expr(e)),
            ExprKind::TupleGet(b, _) => self.expr(b),
        }
    }
}
