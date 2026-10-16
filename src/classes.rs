//! Classes: fields, methods, the constructor `new`, and objects.
//!
//! Classes are lowered onto the existing machinery before type checking.
//! Each method becomes an ordinary subprogram named `Class#method` whose
//! first parameter is `self`, an object of the class. A declared type that
//! names a class (`p as Point`, `list of Point`) is parsed as `Type::Named`
//! and linked here to the class's `Type::Object`. Field access, method calls
//! and `new` are then handled by the checker.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::checker::is_builtin_name;
use crate::errors::{Error, Pos};
use crate::tir::{self, dict_of, list_of, ClassInfo, Type};

/// The separator between a class and its method in a subprogram name. It
/// cannot appear in a user name, so these never clash with real functions.
pub const SEP: char = '#';

/// Lowers every class of the program and links class-named types. On success
/// the class table is installed for the checker and compiler.
pub fn lower(prog: &mut Program) -> Result<(), Vec<Error>> {
    let mut errors = Vec::new();
    let classes = std::mem::take(&mut prog.classes);

    // 1. class ids, with name checks
    let mut ids: HashMap<String, u16> = HashMap::new();
    for c in &classes {
        if is_builtin_name(&c.name) || is_base_type_word(&c.name) {
            errors.push(Error::new(c.pos, format!("\"{}\" is a reserved name: choose another for the class", c.name)));
        } else if ids.insert(c.name.clone(), ids.len() as u16).is_some() {
            errors.push(Error::new(c.pos, format!("class \"{}\" is defined twice", c.name)));
        }
    }
    let resolve = |t: Type, pos: Pos, errs: &mut Vec<Error>| resolve_named(t, &ids, pos, errs);

    // 2a. lower each class's own fields and own methods, and resolve parents
    let n = classes.len();
    let mut own_fields: Vec<Vec<(String, Type)>> = Vec::with_capacity(n);
    let mut own_methods: Vec<Vec<(String, usize)>> = Vec::with_capacity(n);
    let mut parent_of: Vec<Option<u16>> = vec![None; n];
    for (ci, c) in classes.iter().enumerate() {
        let cid = ci as u16;
        let mut fields: Vec<(String, Type)> = Vec::new();
        let mut seen = HashSet::new();
        for d in &c.fields {
            if d.name == "self" {
                errors.push(Error::new(d.pos, "\"self\" is reserved and cannot be a field"));
            } else if matches!(d.shape, Shape::Array(_) | Shape::Matrix(..)) {
                errors.push(
                    Error::new(d.pos, "a field cannot be a fixed array or matrix")
                        .with_hint("use a list, like movements as list of float"),
                );
            } else if !seen.insert(d.name.clone()) {
                errors.push(Error::new(d.pos, format!("field \"{}\" is declared twice in class \"{}\"", d.name, c.name)));
            }
            fields.push((d.name.clone(), resolve(d.full, d.pos, &mut errors)));
        }
        own_fields.push(fields);

        let mut methods: Vec<(String, usize)> = Vec::new();
        let mut method_names = HashSet::new();
        for m in &c.methods {
            if !method_names.insert(m.name.clone()) {
                errors.push(Error::new(m.pos, format!("method \"{}\" is defined twice in class \"{}\"", m.name, c.name)));
            }
            if m.name == "new" && m.is_function {
                errors.push(Error::new(m.pos, "the constructor \"new\" must be a procedure (it has no return type)"));
            }
            tir::method_slot(&m.name);
            let index = prog.subprograms.len();
            methods.push((m.name.clone(), index));
            prog.subprograms.push(lower_method(m, &c.name, cid, &resolve, &mut errors));
        }
        own_methods.push(methods);

        if let Some((pname, ppos)) = &c.parent {
            match ids.get(pname) {
                Some(&pid) if pid != cid => parent_of[ci] = Some(pid),
                Some(_) => errors.push(Error::new(*ppos, format!("class \"{}\" cannot inherit from itself", c.name))),
                None => errors.push(Error::new(*ppos, format!("unknown parent class \"{}\"", pname))),
            }
        }
    }

    // 2b. assemble each class with its parent's fields and methods, parents first
    let mut infos: Vec<Option<ClassInfo>> = vec![None; n];
    loop {
        let mut progress = false;
        for ci in 0..n {
            if infos[ci].is_some() {
                continue;
            }
            let parent = parent_of[ci];
            if let Some(pid) = parent {
                if infos[pid as usize].is_none() {
                    continue; // wait for the parent
                }
            }
            let name = classes[ci].name.clone();
            let (mut fields, mut methods) = match parent {
                Some(pid) => {
                    let p = infos[pid as usize].as_ref().expect("parent built");
                    (p.fields.clone(), p.methods.clone())
                }
                None => (Vec::new(), Vec::new()),
            };
            let inherited_fields: HashSet<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            for f in &own_fields[ci] {
                if inherited_fields.contains(&f.0) {
                    errors.push(Error::new(classes[ci].pos, format!("field \"{}\" is already inherited by \"{}\"", f.0, name)));
                }
                fields.push(f.clone());
            }
            for (mname, mindex) in &own_methods[ci] {
                match methods.iter_mut().find(|(n, _)| n == mname) {
                    Some(slot) => {
                        // the constructor is called on an exact class, never dispatched,
                        // so it may take different parameters from the parent's
                        if mname != "new" {
                            check_override_signature(prog, slot.1, *mindex, mname, &name, classes[ci].pos, &mut errors);
                        }
                        slot.1 = *mindex; // override
                    }
                    None => methods.push((mname.clone(), *mindex)),
                }
            }
            infos[ci] = Some(ClassInfo { name, parent, fields, methods });
            progress = true;
        }
        if infos.iter().all(Option::is_some) {
            break;
        }
        if !progress {
            for ci in 0..n {
                if infos[ci].is_none() {
                    errors.push(Error::new(classes[ci].pos, format!("class \"{}\" is part of an inheritance cycle", classes[ci].name)));
                }
            }
            break;
        }
    }
    let infos: Vec<ClassInfo> = infos.into_iter().flatten().collect();

    // 3. link class-named types everywhere else
    for d in prog.globals.iter_mut().chain(&mut prog.locals) {
        d.full = resolve(d.full, d.pos, &mut errors);
        forbid_self(d, &mut errors);
    }
    for s in &mut prog.subprograms {
        if let Some(ret) = s.ret {
            s.ret = Some(resolve(ret, s.pos, &mut errors));
        }
        for d in s.params.iter_mut().chain(&mut s.locals) {
            d.full = resolve(d.full, d.pos, &mut errors);
            // the implicit self of a method is allowed; a user-written one is not
            if d.name == "self" && !matches!(d.full, Type::Object(_)) {
                errors.push(Error::new(d.pos, "\"self\" is reserved: it names the current object inside a method"));
            }
        }
    }

    if errors.is_empty() {
        tir::set_classes(infos);
        Ok(())
    } else {
        Err(errors)
    }
}

/// An overriding method must keep the parent's parameter types and return type.
fn check_override_signature(
    prog: &Program,
    parent_fn: usize,
    child_fn: usize,
    method: &str,
    class: &str,
    pos: Pos,
    errors: &mut Vec<Error>,
) {
    let (p, c) = (&prog.subprograms[parent_fn], &prog.subprograms[child_fn]);
    let params = |s: &Subprogram| s.params[1..].iter().map(|d| d.full).collect::<Vec<_>>();
    if params(p) != params(c) || p.ret != c.ret {
        errors.push(
            Error::new(pos, format!("method \"{}\" in class \"{}\" must match the one it overrides", method, class))
                .with_hint("keep the same parameter types and return type as in the parent class"),
        );
    }
}

fn lower_method(
    m: &Subprogram,
    class: &str,
    cid: u16,
    resolve: &impl Fn(Type, Pos, &mut Vec<Error>) -> Type,
    errors: &mut Vec<Error>,
) -> Subprogram {
    let self_param = Decl {
        name: "self".to_string(),
        shape: Shape::Scalar,
        ty: tir::BaseType::Int,
        full: Type::Object(cid),
        default: None,
        pos: m.pos,
    };
    let mut params = Vec::with_capacity(m.params.len() + 1);
    params.push(self_param);
    for d in &m.params {
        let mut d = d.clone();
        d.full = resolve(d.full, d.pos, errors);
        params.push(d);
    }
    let ret = m.ret.map(|t| resolve(t, m.pos, errors));
    Subprogram {
        name: format!("{}{}{}", class, SEP, m.name),
        is_function: m.is_function,
        params,
        ret,
        locals: m.locals.clone(),
        body: m.body.clone(),
        pos: m.pos,
    }
}

fn forbid_self(d: &Decl, errors: &mut Vec<Error>) {
    if d.name == "self" {
        errors.push(Error::new(d.pos, "\"self\" is reserved: it names the current object inside a method"));
    }
}

/// Replaces `Type::Named` by the `Type::Object` of the class it names,
/// recursively inside lists and dictionaries.
fn resolve_named(t: Type, ids: &HashMap<String, u16>, pos: Pos, errors: &mut Vec<Error>) -> Type {
    match t {
        Type::Named(id) => {
            let name = tir::named_name(id);
            match ids.get(&name) {
                Some(&cid) => Type::Object(cid),
                None => {
                    errors.push(
                        Error::new(pos, format!("unknown type \"{}\"", name))
                            .with_hint("the types are int, float, string, bool, a list, a dictionary or a class"),
                    );
                    Type::Unknown
                }
            }
        }
        Type::ListOf(id) => list_of(resolve_named(tir::resolve(id), ids, pos, errors)),
        Type::Dict(k, id) => dict_of(k, resolve_named(tir::resolve(id), ids, pos, errors)),
        other => other,
    }
}

fn is_base_type_word(w: &str) -> bool {
    matches!(w, "int" | "float" | "string" | "bool")
}
