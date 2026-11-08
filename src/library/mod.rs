//! The standard library: conversions, math, strings, collections, files and
//! system functions, with Python-like behavior and English names.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

#[cfg(feature = "native")]
use rayon::prelude::*;
use rustfft::{num_complex::Complex, FftPlanner};
use serde_json::Value as Json;
use wide::f64x4;

use crate::builtins::{Builtin, CheckError, Checked};
use crate::tir::BaseType::{Bool, Int, Real, Str};
use crate::tir::{fits, join, list_elem, list_of, resolve, BaseType, Type};
use crate::vm::{display, format_real, missing_key, Dict, Key as DictKey, Value};

// Messages for modules that need a desktop OS and are off in the wasm playground.
#[cfg(not(feature = "native"))]
const NET_OFF: &str = "networking is not available in the web playground";
#[cfg(not(feature = "native"))]
const DB_OFF: &str = "the database is not available in the web playground";
#[cfg(not(feature = "native"))]
const SERVE_OFF: &str = "the HTTP server is not available in the web playground";

// Sequential stand-ins for the few rayon methods used below; the wasm build has
// no threads, so `par_iter`/`into_par_iter` fall back to plain iterators.
#[cfg(not(feature = "native"))]
mod seq_par {
    pub trait ParIter<'a> {
        type Iter: Iterator;
        fn par_iter(self) -> Self::Iter;
    }
    impl<'a, T: 'a> ParIter<'a> for &'a [T] {
        type Iter = std::slice::Iter<'a, T>;
        fn par_iter(self) -> Self::Iter {
            self.iter()
        }
    }
    pub trait IntoParIter {
        type Iter: Iterator;
        fn into_par_iter(self) -> Self::Iter;
    }
    impl IntoParIter for std::ops::Range<usize> {
        type Iter = std::ops::Range<usize>;
        fn into_par_iter(self) -> Self::Iter {
            self
        }
    }
}
#[cfg(not(feature = "native"))]
use seq_par::{IntoParIter, ParIter};

// ---------------------------------------------------------------- types

/// Element type of an array or list argument.
fn collection(t: Type) -> Option<BaseType> {
    match t {
        Type::Array(e) | Type::List(e) => e,
        _ => None,
    }
}

fn numeric(b: BaseType) -> bool {
    matches!(b, Int | Real)
}

fn ok(convert: Vec<Option<BaseType>>, ret: Type) -> Result<Checked, CheckError> {
    Ok(Checked { convert, ret })
}

fn other<T>(msg: impl Into<String>) -> Result<T, CheckError> {
    Err(CheckError::Other(msg.into()))
}

/// Type rules of the built-ins that accept more than one combination of types.
pub fn check(b: Builtin, a: &[Type]) -> Result<Checked, CheckError> {
    use Builtin::*;
    let none = |n: usize| vec![None; n];
    if a.contains(&Type::Unknown) {
        // an argument already has an error: accept the call quietly
        return ok(none(a.len()), Type::Unknown);
    }
    let arity = |n: usize| if a.len() == n { Ok(()) } else { Err(CheckError::Arity(n)) };
    match b {
        ToInt => {
            arity(1)?;
            match a[0] {
                Type::Base(_) => ok(none(1), Type::Base(Int)),
                _ => other("to_int works with a string, a number or a bool"),
            }
        }
        ToFloat => {
            arity(1)?;
            match a[0] {
                Type::Base(Str | Int | Real) => ok(none(1), Type::Base(Real)),
                _ => other("to_float works with a string or a number"),
            }
        }
        ToText => {
            arity(1)?;
            match a[0] {
                _ => ok(none(1), Type::Base(Str)),
            }
        }
        TaskSpawn => {
            arity(1)?;
            match a[0] {
                Type::Func(_) => ok(none(1), Type::Base(Int)),
                _ => other("task.spawn needs a function to run, like task.spawn(worker)"),
            }
        }
        AiEmbed => {
            arity(1)?;
            match a[0] {
                Type::Base(Str) => ok(none(1), Type::Grid),
                _ => other("ai.embed needs a text, like ai.embed(\"hello world\")"),
            }
        }
        AiModels => {
            arity(0)?;
            ok(none(0), Type::List(Some(Str)))
        }
        Absolute => {
            arity(1)?;
            match a[0] {
                Type::Base(t) if numeric(t) => ok(none(1), Type::Base(t)),
                _ => other("absolute works with a number"),
            }
        }
        Minimum | Maximum => match a {
            [Type::Base(x), Type::Base(y)] if numeric(*x) && numeric(*y) => {
                let t = if *x == Int && *y == Int { Int } else { Real };
                ok(vec![Some(t), Some(t)], Type::Base(t))
            }
            [c] if collection(*c).is_some_and(|e| e != Bool) => ok(none(1), Type::Base(collection(*c).unwrap())),
            _ => other(format!("{} works with two numbers, or with an array or list", b.name())),
        },
        Logarithm => match a {
            [Type::Base(x)] if numeric(*x) => ok(vec![Some(Real)], Type::Base(Real)),
            [Type::Base(x), Type::Base(y)] if numeric(*x) && numeric(*y) => {
                ok(vec![Some(Real), Some(Real)], Type::Base(Real))
            }
            _ => other(format!("logarithm takes 1 or 2 numbers, here it gets {} value(s)", a.len())),
        },
        Split => {
            arity(2)?;
            for (index, t) in a.iter().enumerate() {
                if *t != Type::Base(Str) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Str) });
                }
            }
            ok(none(2), Type::List(Some(Str)))
        }
        ReadLines => {
            arity(1)?;
            if a[0] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Str) });
            }
            ok(none(1), Type::List(Some(Str)))
        }
        Join => {
            arity(2)?;
            if collection(a[0]).is_none() {
                return other("join works with an array or list and a separator string");
            }
            if a[1] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Str) });
            }
            ok(none(2), Type::Base(Str))
        }
        Add | Discard => {
            arity(2)?;
            let Type::Set(e) = a[0] else { return other(format!("{} works with a set", b.name())) };
            if !fits(Type::Base(e), a[1]) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(e) });
            }
            ok(vec![None, Some(e)], Type::Unknown)
        }
        ToList => {
            arity(1)?;
            match a[0] {
                Type::Set(e) => ok(none(1), Type::List(Some(e))),
                _ => other("to_list works with a set"),
            }
        }
        Contains | IndexOf => {
            arity(2)?;
            let ret = Type::Base(if b == Contains { Bool } else { Int });
            match (a[0], a[1]) {
                (Type::Set(e), Type::Base(v)) if b == Contains && join(e, v) == Some(e) => ok(vec![None, Some(e)], ret),
                (Type::Base(Str), Type::Base(Str)) if b == Contains => ok(none(2), ret),
                (c, Type::Base(v)) if collection(c).is_some() => {
                    let e = collection(c).unwrap();
                    if join(e, v) == Some(e) {
                        ok(vec![None, Some(e)], ret)
                    } else {
                        other(format!("the value to look for must be {}, here it is {}", e.name(), v.name()))
                    }
                }
                _ if b == Contains => other("contains works with a string and a part, or with an array or list and a value"),
                _ => other("index_of works with an array or list and a value"),
            }
        }
        Sum => {
            arity(1)?;
            match collection(a[0]) {
                Some(e) if numeric(e) => ok(none(1), Type::Base(e)),
                _ => other("sum works with an array or list of numbers"),
            }
        }
        Sort | Reverse => {
            arity(1)?;
            match (collection(a[0]), list_elem(a[0])) {
                (Some(_), _) => ok(none(1), Type::Unknown),
                (None, Some(_)) if b == Reverse => ok(none(1), Type::Unknown),
                (None, Some(_)) => other("sort works only with lists of simple values (ints, floats, strings or bools)"),
                _ => other(format!("{} works with an array or list", b.name())),
            }
        }
        Insert => {
            arity(3)?;
            let Some(e) = list_elem(a[0]) else { return other("insert works with a list") };
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            if !fits(e, a[2]) {
                return other(format!("the value to insert must be {}, here it is {}", e.full_name(), a[2].full_name()));
            }
            ok(vec![None, None, e.base()], Type::Unknown)
        }
        TakeLast | ClearList => {
            arity(1)?;
            match list_elem(a[0]) {
                Some(e) => ok(none(1), if b == TakeLast { e } else { Type::Unknown }),
                None => other(format!("{} works with a list", b.name())),
            }
        }
        HasKey | Get | RemoveKey => {
            arity(if b == Get { 3 } else { 2 })?;
            let Type::Dict(k, v) = a[0] else { return other(format!("{} works with a dictionary", b.name())) };
            if !fits(Type::Base(k), a[1]) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(k) });
            }
            let v = resolve(v);
            match b {
                HasKey => ok(none(2), Type::Base(Bool)),
                RemoveKey => ok(none(2), Type::Unknown),
                _ if fits(v, a[2]) => ok(vec![None, None, v.base()], v),
                _ => Err(CheckError::Arg { index: 2, expected: v }),
            }
        }
        Keys | Values => {
            arity(1)?;
            match a[0] {
                Type::Dict(k, _) if b == Keys => ok(none(1), Type::List(Some(k))),
                Type::Dict(_, v) => ok(none(1), list_of(resolve(v))),
                _ => other(format!("{} works with a dictionary", b.name())),
            }
        }
        TextAll => {
            arity(2)?;
            match (a[0], a[1]) {
                (Type::Base(Str), Type::Base(Str)) => ok(none(2), Type::List(Some(Str))),
                _ => other("text.all needs a string and a pattern"),
            }
        }
        DataKeys => {
            arity(2)?;
            for (index, t) in a.iter().enumerate() {
                if *t != Type::Base(Str) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Str) });
                }
            }
            ok(none(2), Type::List(Some(Str)))
        }
        DataToJson => {
            arity(1)?;
            // any value can be turned into JSON; a bad one fails at run time
            ok(none(1), Type::Base(Str))
        }
        DirList | CsvFields => {
            arity(1)?;
            if a[0] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Str) });
            }
            ok(none(1), Type::List(Some(Str)))
        }
        CsvLine => {
            arity(1)?;
            match collection(a[0]) {
                Some(Str) => ok(none(1), Type::Base(Str)),
                _ => other("csv.line works with an array or list of strings"),
            }
        }
        StatsMean | StatsMedian | StatsStdev => {
            arity(1)?;
            match collection(a[0]) {
                Some(e) if numeric(e) => ok(none(1), Type::Base(Real)),
                _ => other(format!("{} works with an array or list of numbers", b.name())),
            }
        }
        GridRange | GridZeros | GridRand | GridRandn | GridEye => {
            arity(1)?;
            if a[0] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Int) });
            }
            ok(none(1), Type::Grid)
        }
        GridFill => {
            arity(2)?;
            if a[0] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Int) });
            }
            if a[1].base().map_or(true, |t| !numeric(t)) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Real) });
            }
            ok(vec![None, Some(Real)], Type::Grid)
        }
        GridOf => {
            arity(1)?;
            match collection(a[0]) {
                Some(e) if numeric(e) => ok(none(1), Type::Grid),
                _ => other("nungw.of works with an array or list of numbers"),
            }
        }
        GridSize => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::Base(Int))
        }
        FfiOpen => {
            arity(1)?;
            if a[0] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Str) });
            }
            ok(none(1), Type::Base(Int))
        }
        FfiInt | FfiInt32 | FfiReal => {
            arity(3)?;
            if a[0] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Int) });
            }
            if a[1] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Str) });
            }
            match collection(a[2]) {
                Some(e) if numeric(e) => ok(none(3), if b == FfiReal { Type::Base(Real) } else { Type::Base(Int) }),
                _ => other("the third argument must be a list of numbers"),
            }
        }
        FfiBuffer => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::Base(Int))
        }
        FfiCallInt | FfiCallReal => {
            arity(4)?;
            if a[0] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Int) });
            }
            if a[1] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Str) });
            }
            if a[2] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 2, expected: Type::Base(Str) });
            }
            match collection(a[3]) {
                Some(e) if numeric(e) => ok(none(4), if b == FfiCallReal { Type::Base(Real) } else { Type::Base(Int) }),
                _ => other("the fourth argument must be a list of numbers"),
            }
        }
        GridGet => {
            arity(2)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            ok(none(2), Type::Base(Real))
        }
        GridSet => {
            arity(3)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            if a[2].base().map_or(true, |t| !numeric(t)) {
                return Err(CheckError::Arg { index: 2, expected: Type::Base(Real) });
            }
            ok(vec![None, None, Some(Real)], Type::Unknown)
        }
        GridFree => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::Unknown)
        }
        GridToList => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::List(Some(Real)))
        }
        GridAdd | GridSub | GridMul | GridDiv | GridDot
        | GridPow | GridAtan2 | GridHypot | GridMod | GridMaximum | GridMinimum | GridConcat => {
            arity(2)?;
            for (index, t) in a.iter().enumerate() {
                if *t != Type::Grid {
                    return Err(CheckError::Arg { index, expected: Type::Grid });
                }
            }
            ok(none(2), if b == GridDot { Type::Base(Real) } else { Type::Grid })
        }
        GridScale | GridOffset | GridSubScalar | GridScalarSub | GridDivScalar | GridScalarDiv => {
            arity(2)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            if a[1].base().map_or(true, |t| !numeric(t)) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Real) });
            }
            ok(vec![None, Some(Real)], Type::Grid)
        }
        GridSqrt | GridSquare | GridAbs
        | GridExp | GridExp2 | GridExpm1
        | GridLog | GridLog2 | GridLog10 | GridLog1p
        | GridSin | GridCos | GridTan | GridAsin | GridAcos | GridAtan
        | GridSinh | GridCosh | GridTanh | GridAsinh | GridAcosh | GridAtanh
        | GridFloor | GridCeil | GridRound | GridTrunc
        | GridSign | GridCbrt | GridRecip | GridNeg | GridDegrees | GridRadians
        | GridFft | GridIfft | GridCabs
        | GridCumsum | GridCumprod | GridSort | GridArgsort => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::Grid)
        }
        GridSum | GridMean | GridMax | GridMin | GridSumSquares | GridNorm
        | GridProd | GridVar | GridStd | GridMedian | GridArgmax | GridArgmin | GridPtp => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::Base(Real))
        }
        GridEval => ok(none(a.len()), Type::Grid), // compiler-generated; its arguments are already typed
        CounterItems => {
            arity(1)?;
            match collection(a[0]) {
                Some(Str) => ok(none(1), crate::tir::dict_of(Str, Type::Base(Int))),
                _ => other("counter.items works with an array or list of strings"),
            }
        }
        CounterChars | CounterWords => {
            arity(1)?;
            if a[0] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Str) });
            }
            ok(none(1), crate::tir::dict_of(Str, Type::Base(Int)))
        }
        TextSplit => {
            arity(2)?;
            for (index, t) in a.iter().enumerate() {
                if *t != Type::Base(Str) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Str) });
                }
            }
            ok(none(2), Type::List(Some(Str)))
        }
        ChartLine | ChartBars => {
            arity(2)?;
            if collection(a[0]).map_or(true, |e| !numeric(e)) {
                return other("chart.line/bars works with a list of numbers and a file path");
            }
            if a[1] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Str) });
            }
            ok(none(2), Type::Unknown)
        }
        ChartScatter => {
            arity(3)?;
            for index in [0usize, 1] {
                if collection(a[index]).map_or(true, |e| !numeric(e)) {
                    return other("chart.scatter works with two lists of numbers and a file path");
                }
            }
            if a[2] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 2, expected: Type::Base(Str) });
            }
            ok(none(3), Type::Unknown)
        }
        TableHeaders => {
            arity(1)?;
            if a[0] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Str) });
            }
            ok(none(1), Type::List(Some(Str)))
        }
        TableColumn | TableNumbers => {
            arity(2)?;
            for (index, t) in a.iter().enumerate() {
                if *t != Type::Base(Str) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Str) });
                }
            }
            ok(none(2), Type::List(Some(if b == TableNumbers { Real } else { Str })))
        }
        IterCombinations | IterPermutations => {
            arity(2)?;
            if a[0] != Type::Base(Str) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Str) });
            }
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            ok(none(2), Type::List(Some(Str)))
        }
        IterProduct => {
            arity(2)?;
            for (index, t) in a.iter().enumerate() {
                if *t != Type::Base(Str) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Str) });
                }
            }
            ok(none(2), Type::List(Some(Str)))
        }
        PickShuffle => {
            arity(1)?;
            match collection(a[0]) {
                Some(_) => ok(none(1), Type::Unknown),
                None => other("pick.shuffle works with an array or list"),
            }
        }
        PickChoice => {
            arity(1)?;
            match collection(a[0]) {
                Some(e) => ok(none(1), Type::Base(e)),
                None => other("pick.choice works with an array or list"),
            }
        }
        PickSample => {
            arity(2)?;
            let Some(e) = collection(a[0]) else { return other("pick.sample works with an array or list and a count") };
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            ok(none(2), Type::List(Some(e)))
        }
        GridMatmul | GridMatmulTA | GridMatmulTB => {
            // shape-aware form f(a, b): dimensions read from the 2-D shapes.
            // Or the explicit form f(a, rows, inner, b, cols) for flat arrays.
            if a.len() == 2 {
                for index in [0usize, 1] {
                    if a[index] != Type::Grid {
                        return Err(CheckError::Arg { index, expected: Type::Grid });
                    }
                }
                return ok(none(2), Type::Grid);
            }
            arity(5)?;
            for (index, t) in [(0usize, a[0]), (3, a[3])] {
                if t != Type::Grid {
                    return Err(CheckError::Arg { index, expected: Type::Grid });
                }
            }
            for index in [1usize, 2, 4] {
                if a[index] != Type::Base(Int) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Int) });
                }
            }
            ok(none(5), Type::Grid)
        }
        GridGpu => {
            arity(1)?;
            if a[0] != Type::Base(Bool) {
                return Err(CheckError::Arg { index: 0, expected: Type::Base(Bool) });
            }
            ok(none(1), Type::Base(Bool))
        }
        GridToGpu | GridToCpu => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::Grid)
        }
        GridInv | GridDet | GridEigvalsSym | GridEigvecsSym => {
            // shape-aware form f(a) using a's square 2-D shape, or explicit f(a, n).
            if a.len() == 1 {
                if a[0] != Type::Grid {
                    return Err(CheckError::Arg { index: 0, expected: Type::Grid });
                }
                return ok(none(1), if b == GridDet { Type::Base(Real) } else { Type::Grid });
            }
            arity(2)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            ok(none(2), if b == GridDet { Type::Base(Real) } else { Type::Grid })
        }
        GridSolve => {
            // shape-aware form solve(a, b) using a's shape, or explicit solve(a, n, b).
            if a.len() == 2 {
                for index in [0usize, 1] {
                    if a[index] != Type::Grid {
                        return Err(CheckError::Arg { index, expected: Type::Grid });
                    }
                }
                return ok(none(2), Type::Grid);
            }
            arity(3)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            if a[2] != Type::Grid {
                return Err(CheckError::Arg { index: 2, expected: Type::Grid });
            }
            ok(none(3), Type::Grid)
        }
        GridTranspose => {
            // shape-aware form transpose(a), or explicit transpose(a, rows, cols).
            if a.len() == 1 {
                if a[0] != Type::Grid {
                    return Err(CheckError::Arg { index: 0, expected: Type::Grid });
                }
                return ok(none(1), Type::Grid);
            }
            arity(3)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            for index in [1usize, 2] {
                if a[index] != Type::Base(Int) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Int) });
                }
            }
            ok(none(3), Type::Grid)
        }
        GridSingularValues | GridSvdU | GridSvdV => {
            // shape-aware form f(a), or explicit (a, m, n).
            if a.len() == 1 {
                if a[0] != Type::Grid {
                    return Err(CheckError::Arg { index: 0, expected: Type::Grid });
                }
                return ok(none(1), Type::Grid);
            }
            arity(3)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            for index in [1usize, 2] {
                if a[index] != Type::Base(Int) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Int) });
                }
            }
            ok(none(3), Type::Grid)
        }
        GridSlice | GridRows => {
            arity(3)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            for index in [1usize, 2] {
                if a[index] != Type::Base(Int) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Int) });
                }
            }
            ok(none(3), Type::Grid)
        }
        GridConcatAxis => {
            arity(3)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            if a[1] != Type::Grid {
                return Err(CheckError::Arg { index: 1, expected: Type::Grid });
            }
            if a[2] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 2, expected: Type::Base(Int) });
            }
            ok(none(3), Type::Grid)
        }
        GridLstsq => {
            // shape-aware form lstsq(a, b) using a's shape, or explicit (a, m, n, b).
            if a.len() == 2 {
                for index in [0usize, 1] {
                    if a[index] != Type::Grid {
                        return Err(CheckError::Arg { index, expected: Type::Grid });
                    }
                }
                return ok(none(2), Type::Grid);
            }
            arity(4)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            for index in [1usize, 2] {
                if a[index] != Type::Base(Int) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Int) });
                }
            }
            if a[3] != Type::Grid {
                return Err(CheckError::Arg { index: 3, expected: Type::Grid });
            }
            ok(none(4), Type::Grid)
        }
        GridShape => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::List(Some(Int)))
        }
        GridNdim => {
            arity(1)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            ok(none(1), Type::Base(Int))
        }
        GridReshape => {
            arity(2)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            match collection(a[1]) {
                Some(e) if numeric(e) => ok(none(2), Type::Grid),
                _ => other("nungw.reshape needs a nungw and a list of sizes"),
            }
        }
        GridSumAxis | GridMeanAxis | GridMaxAxis | GridMinAxis | GridProdAxis => {
            arity(2)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            if a[1] != Type::Base(Int) {
                return Err(CheckError::Arg { index: 1, expected: Type::Base(Int) });
            }
            ok(none(2), Type::Grid)
        }
        GridAt => {
            if a.len() < 2 {
                return Err(CheckError::Arity(2));
            }
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            for index in 1..a.len() {
                if a[index] != Type::Base(Int) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Int) });
                }
            }
            ok(none(a.len()), Type::Base(Real))
        }
        GridClip => {
            arity(3)?;
            if a[0] != Type::Grid {
                return Err(CheckError::Arg { index: 0, expected: Type::Grid });
            }
            for index in [1usize, 2] {
                if a[index].base().map_or(true, |t| !numeric(t)) {
                    return Err(CheckError::Arg { index, expected: Type::Base(Real) });
                }
            }
            ok(vec![None, Some(Real), Some(Real)], Type::Grid)
        }
        GridWhere => {
            arity(3)?;
            for index in 0..3 {
                if a[index] != Type::Grid {
                    return Err(CheckError::Arg { index, expected: Type::Grid });
                }
            }
            ok(none(3), Type::Grid)
        }
        other_builtin => panic!("internal error: {:?} has a fixed signature", other_builtin),
    }
}

// ---------------------------------------------------------------- values

fn int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        other => panic!("internal error: expected an int, found {:?}", other),
    }
}

fn real(v: &Value) -> f64 {
    match v {
        Value::Real(r) => *r,
        Value::Int(n) => *n as f64,
        other => panic!("internal error: expected a number, found {:?}", other),
    }
}

fn text(v: &Value) -> &str {
    match v {
        Value::Str(s) => s,
        other => panic!("internal error: expected a string, found {:?}", other),
    }
}

fn set_data(v: &Value) -> &Rc<RefCell<Dict>> {
    match v {
        Value::Set(d) => d,
        other => panic!("internal error: expected a set, found {:?}", other),
    }
}

fn dict(v: &Value) -> &Rc<RefCell<Dict>> {
    match v {
        Value::Dict(d) => d,
        other => panic!("internal error: expected a dictionary, found {:?}", other),
    }
}

fn new_list(items: Vec<Value>) -> Value {
    Value::Seq(Rc::new(RefCell::new(items)))
}

fn seq(v: &Value) -> &Rc<RefCell<Vec<Value>>> {
    match v {
        Value::Seq(s) => s,
        other => panic!("internal error: expected an array or list, found {:?}", other),
    }
}

fn string(s: impl Into<Rc<str>>) -> Value {
    Value::Str(s.into())
}

/// Compiles a regular expression, turning a bad pattern into a run-time error.
fn pattern(v: &Value) -> Result<regex::Regex, String> {
    regex::Regex::new(text(v)).map_err(|e| format!("invalid pattern: {}", e.to_string().lines().last().unwrap_or("")))
}

/// Lowercase hexadecimal text of a byte sequence (used by the `hash` module).
fn hex(bytes: impl AsRef<[u8]>) -> String {
    let mut s = String::with_capacity(bytes.as_ref().len() * 2);
    for b in bytes.as_ref() {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn list(items: Vec<Value>) -> Value {
    Value::Seq(Rc::new(RefCell::new(items)))
}

/// Fetches a web page over HTTP(S) and returns its body as text (the `web` module).
/// Network, server and timeout problems become ordinary run-time errors, so the
/// program can handle them with `try`/`catch`.
#[cfg(feature = "native")]
fn web_get(url: &str) -> Result<String, String> {
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("web.get: the server answered with status {}", code),
            ureq::Error::Transport(t) => format!("web.get: cannot reach \"{}\": {}", url, t),
        })?;
    response.into_string().map_err(|e| format!("web.get: cannot read the reply: {}", e))
}

#[cfg(not(feature = "native"))]
fn web_get(_url: &str) -> Result<String, String> {
    Err(NET_OFF.to_string())
}

/// Turns a ureq error into a Globw run-time message for a `web.*` call.
#[cfg(feature = "native")]
fn web_error(ctx: &str, url: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("{}: the server answered with status {}", ctx, code),
        ureq::Error::Transport(t) => format!("{}: cannot reach \"{}\": {}", ctx, url, t),
    }
}

// ---- module "ai": chat with an OpenAI-compatible endpoint (helpers in ai.rs) ----
mod ai;
use ai::*;

// ---- module "symbol": symbolic mathematics, a tiny CAS (helpers in symbol.rs) ----
mod symbol;
use symbol::*;

// ---- module "synth": audio synthesis and simple DSP (helpers in synth.rs) ----
mod synth;
use synth::*;

// ---- module "gwsim": a small 2D physics world of circular bodies (helpers in gwsim.rs) ----
mod gwsim;
use gwsim::*;

// ---- module "gwframe": a small numeric dataframe (pandas-like) (helpers in gwframe.rs) ----
mod gwframe;
use gwframe::*;

// ---- module "grad": a tape-based reverse-mode autodiff engine (helpers in grad.rs) ----
mod grad;
use grad::*;

// -------- helpers for the "net"/"web" modules (in net.rs) --------
mod net;
use net::*;

// -------- the "chart" module (SVG plots) (helpers in chart.rs) --------
mod chart;
use chart::*;
// ---- table (CSV) helpers ----

fn csv_lines(csv: &str) -> Vec<&str> {
    csv.lines().filter(|l| !l.trim().is_empty()).collect()
}

fn table_col_index<'a>(csv: &'a str, name: &str) -> Result<(Vec<&'a str>, usize), String> {
    let lines = csv_lines(csv);
    let header = *lines.first().ok_or("table: the CSV is empty")?;
    let idx = csv_fields(header)
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| format!("table: there is no column \"{}\"", name))?;
    Ok((lines, idx))
}

fn table_cell_number(fields: &[String], idx: usize) -> Result<f64, String> {
    let cell = fields.get(idx).map(|s| s.trim()).unwrap_or("");
    cell.parse::<f64>().map_err(|_| format!("table: \"{}\" is not a number", cell))
}

// ---- iter (combinatorics over a string's characters) ----

fn char_combinations(s: &str, k: i64) -> Result<Vec<String>, String> {
    if k < 0 {
        return Err("iter.combinations: the count cannot be negative".to_string());
    }
    let chars: Vec<char> = s.chars().collect();
    let k = k as usize;
    if k == 0 {
        return Ok(vec![String::new()]);
    }
    if k > chars.len() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        out.push(idx.iter().map(|&i| chars[i]).collect::<String>());
        if out.len() > 1_000_000 {
            return Err("iter.combinations: too many results".to_string());
        }
        let mut i = k;
        loop {
            if i == 0 {
                return Ok(out);
            }
            i -= 1;
            if idx[i] != i + chars.len() - k {
                break;
            }
        }
        idx[i] += 1;
        for j in i + 1..k {
            idx[j] = idx[j - 1] + 1;
        }
    }
}

fn char_permutations(s: &str, k: i64) -> Result<Vec<String>, String> {
    if k < 0 {
        return Err("iter.permutations: the count cannot be negative".to_string());
    }
    let chars: Vec<char> = s.chars().collect();
    let k = k as usize;
    if k > chars.len() {
        return Ok(vec![]);
    }
    fn rec(chars: &[char], k: usize, used: &mut [bool], cur: &mut Vec<char>, out: &mut Vec<String>) -> Result<(), String> {
        if cur.len() == k {
            out.push(cur.iter().collect());
            if out.len() > 1_000_000 {
                return Err("iter.permutations: too many results".to_string());
            }
            return Ok(());
        }
        for i in 0..chars.len() {
            if !used[i] {
                used[i] = true;
                cur.push(chars[i]);
                rec(chars, k, used, cur, out)?;
                cur.pop();
                used[i] = false;
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    let mut used = vec![false; chars.len()];
    let mut cur = Vec::new();
    rec(&chars, k, &mut used, &mut cur, &mut out)?;
    Ok(out)
}

fn char_product(a: &str, b: &str) -> Vec<String> {
    let mut out = Vec::new();
    for x in a.chars() {
        for y in b.chars() {
            out.push(format!("{}{}", x, y));
        }
    }
    out
}

// ---- batch G1 helpers ----

fn beaufort(kmh: f64) -> i64 {
    const T: [f64; 12] = [1.0, 6.0, 12.0, 20.0, 29.0, 39.0, 50.0, 62.0, 75.0, 89.0, 103.0, 118.0];
    T.iter().filter(|&&t| kmh >= t).count() as i64
}

fn erf(x: f64) -> f64 {
    // Abramowitz & Stegun 7.1.26
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t + 0.254_829_592)
            * t
            * (-x * x).exp();
    if x < 0.0 {
        -y
    } else {
        y
    }
}

fn gamma(x: f64) -> f64 {
    // Lanczos approximation (g = 7)
    const G: [f64; 9] = [
        0.999_999_999_999_809_9, 676.520_368_121_885_1, -1259.139_216_722_402_8, 771.323_428_777_653_1,
        -176.615_029_162_140_6, 12.507_343_278_686_905, -0.138_571_095_265_720_12, 9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma(1.0 - x))
    } else {
        let x = x - 1.0;
        let mut a = G[0];
        let t = x + 7.5;
        for (i, &g) in G.iter().enumerate().skip(1) {
            a += g / (x + i as f64);
        }
        std::f64::consts::TAU.sqrt() * t.powf(x + 0.5) * (-t).exp() * a
    }
}

fn rotate_str(s: &str, n: i64) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    if len == 0 {
        return String::new();
    }
    let shift = n.rem_euclid(len as i64) as usize;
    chars[shift..].iter().chain(&chars[..shift]).collect()
}

fn sentence_case(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut ch = lower.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

fn dedup_chars(s: &str) -> String {
    let mut out = String::new();
    let mut prev = None;
    for c in s.chars() {
        if Some(c) != prev {
            out.push(c);
            prev = Some(c);
        }
    }
    out
}

fn hamming(a: &str, b: &str) -> Result<i64, String> {
    let (ac, bc): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if ac.len() != bc.len() {
        return Err("hamming.distance: the strings must have the same length".to_string());
    }
    Ok(ac.iter().zip(&bc).filter(|(x, y)| x != y).count() as i64)
}

fn levenshtein(a: &str, b: &str) -> i64 {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let (m, n) = (ac.len(), bc.len());
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut cur = vec![0usize; n + 1];
    for i in 1..=m {
        cur[0] = i;
        for j in 1..=n {
            let cost = usize::from(ac[i - 1] != bc[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[n] as i64
}

fn soundex(s: &str) -> String {
    let code = |c: char| match c.to_ascii_uppercase() {
        'B' | 'F' | 'P' | 'V' => '1',
        'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => '2',
        'D' | 'T' => '3',
        'L' => '4',
        'M' | 'N' => '5',
        'R' => '6',
        _ => '0',
    };
    let letters: Vec<char> = s.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if letters.is_empty() {
        return "0000".to_string();
    }
    let first = letters[0].to_ascii_uppercase();
    let mut out = String::from(first);
    let mut prev = code(first);
    for &c in &letters[1..] {
        let d = code(c);
        if d != '0' && d != prev {
            out.push(d);
            if out.len() == 4 {
                break;
            }
        }
        if !matches!(c.to_ascii_uppercase(), 'H' | 'W') {
            prev = d;
        }
    }
    while out.len() < 4 {
        out.push('0');
    }
    out
}

fn syllables(s: &str) -> i64 {
    let mut total = 0;
    for w in s.split_whitespace() {
        let mut count = 0;
        let mut prev_vowel = false;
        for c in w.chars() {
            let v = "aeiouyAEIOUY".contains(c);
            if v && !prev_vowel {
                count += 1;
            }
            prev_vowel = v;
        }
        total += count.max(1);
    }
    total
}

// ---- batch G3 helpers ----

fn abbreviate_number(n: f64) -> String {
    let abs = n.abs();
    let (v, suffix) = if abs >= 1e12 {
        (n / 1e12, "T")
    } else if abs >= 1e9 {
        (n / 1e9, "B")
    } else if abs >= 1e6 {
        (n / 1e6, "M")
    } else if abs >= 1e3 {
        (n / 1e3, "k")
    } else {
        (n, "")
    };
    if suffix.is_empty() {
        format_real(n)
    } else {
        format!("{:.1}{}", v, suffix)
    }
}

fn gray_decode(g: i64) -> i64 {
    let mut n = g;
    let mut m = g >> 1;
    while m != 0 {
        n ^= m;
        m >>= 1;
    }
    n
}

fn ean_check(s: &str) -> Result<i64, String> {
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != 12 {
        return Err("ean.check_digit: it needs exactly 12 digits".to_string());
    }
    let sum: u32 = digits.iter().enumerate().map(|(i, &d)| if i % 2 == 0 { d } else { d * 3 }).sum();
    Ok(i64::from((10 - (sum % 10)) % 10))
}

fn cardtype(number: &str) -> String {
    let d: String = number.chars().filter(|c| c.is_ascii_digit()).collect();
    let two = d.get(..2).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
    (if d.starts_with('4') {
        "Visa"
    } else if (51..=55).contains(&two) {
        "Mastercard"
    } else if d.starts_with("34") || d.starts_with("37") {
        "American Express"
    } else if d.starts_with("6011") || d.starts_with("65") {
        "Discover"
    } else {
        "Unknown"
    })
    .to_string()
}

fn happy_is(n: i64) -> bool {
    if n < 1 {
        return false;
    }
    let mut n = n;
    let mut seen = std::collections::HashSet::new();
    while n != 1 && seen.insert(n) {
        let mut s = 0;
        let mut m = n;
        while m > 0 {
            let d = m % 10;
            s += d * d;
            m /= 10;
        }
        n = s;
    }
    n == 1
}

fn lucas(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("lucas.number: the position cannot be negative".to_string());
    }
    let (mut a, mut b) = (2i64, 1i64);
    for _ in 0..n {
        let t = a.checked_add(b).ok_or(OVERFLOW)?;
        a = b;
        b = t;
    }
    Ok(a)
}

fn tribonacci(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("tribonacci.number: the position cannot be negative".to_string());
    }
    let mut v = [0i64, 1, 1];
    if (n as usize) < 3 {
        return Ok(v[n as usize]);
    }
    for _ in 3..=n {
        let t = v[0].checked_add(v[1]).and_then(|x| x.checked_add(v[2])).ok_or(OVERFLOW)?;
        v = [v[1], v[2], t];
    }
    Ok(v[2])
}

fn padovan(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("padovan.number: the position cannot be negative".to_string());
    }
    let mut v = [1i64, 1, 1];
    if (n as usize) < 3 {
        return Ok(1);
    }
    for _ in 3..=n {
        let t = v[0].checked_add(v[1]).ok_or(OVERFLOW)?;
        v = [v[1], v[2], t];
    }
    Ok(v[2])
}

fn primorial(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("primorial.of: the count cannot be negative".to_string());
    }
    let mut prod = 1i64;
    let mut count = 0;
    let mut c = 1;
    while count < n {
        c += 1;
        if is_prime(c) {
            prod = prod.checked_mul(c).ok_or(OVERFLOW)?;
            count += 1;
        }
    }
    Ok(prod)
}

fn primecount(n: i64) -> i64 {
    (2..n).filter(|&k| is_prime(k)).count() as i64
}

fn prevprime(n: i64) -> Result<i64, String> {
    let mut c = n - 1;
    while c >= 2 {
        if is_prime(c) {
            return Ok(c);
        }
        c -= 1;
    }
    Err("prevprime.before: there is no prime below this".to_string())
}

fn lcm2(a: i64, b: i64) -> Result<i64, String> {
    let g = gcd(a, b);
    if g == 0 {
        return Ok(0);
    }
    (a / g).checked_mul(b).map(i64::abs).ok_or_else(|| OVERFLOW.to_string())
}

fn lcm3(a: i64, b: i64, c: i64) -> Result<i64, String> {
    lcm2(lcm2(a, b)?, c)
}

fn a1z26_encode(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_ascii_alphabetic())
                .map(|c| (c.to_ascii_lowercase() as u8 - b'a' + 1).to_string())
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn a1z26_decode(s: &str) -> Result<String, String> {
    let mut words = Vec::new();
    for w in s.split_whitespace() {
        let mut word = String::new();
        for tok in w.split('-').filter(|t| !t.is_empty()) {
            let n: u8 = tok.parse().map_err(|_| format!("a1z26.decode: \"{}\" is not a number", tok))?;
            if !(1..=26).contains(&n) {
                return Err("a1z26.decode: the numbers must be 1 to 26".to_string());
            }
            word.push((b'a' + n - 1) as char);
        }
        words.push(word);
    }
    Ok(words.join(" "))
}

fn revbits(n: i64, width: i64) -> Result<i64, String> {
    if !(1..=63).contains(&width) {
        return Err("revbits.of: the width must be from 1 to 63".to_string());
    }
    let n = n as u64;
    let mut r = 0i64;
    for i in 0..width {
        r |= (((n >> i) & 1) as i64) << (width - 1 - i);
    }
    Ok(r)
}

fn parse_hexcolor(s: &str) -> Result<(u8, u8, u8), String> {
    let t = s.trim();
    let h = t.strip_prefix('#').unwrap_or(t);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("not a #rrggbb color: \"{}\"", s));
    }
    let n = u32::from_str_radix(h, 16).unwrap();
    Ok(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn lum_perceived(s: &str) -> Result<f64, String> {
    let (r, g, b) = parse_hexcolor(s)?;
    Ok(0.2126 * r as f64 / 255.0 + 0.7152 * g as f64 / 255.0 + 0.0722 * b as f64 / 255.0)
}

fn rel_lum(s: &str) -> Result<f64, String> {
    let (r, g, b) = parse_hexcolor(s)?;
    let lin = |c: u8| {
        let v = c as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    Ok(0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b))
}

// ---- the "db" module (SQLite) ----

#[cfg(feature = "native")]
thread_local! {
    static DBS: RefCell<Vec<Option<rusqlite::Connection>>> = const { RefCell::new(Vec::new()) };
}

// ---- the "serve" module (HTTP server) ----

#[cfg(feature = "native")]
struct ServerState {
    server: tiny_http::Server,
    pending: Option<tiny_http::Request>,
}

#[cfg(feature = "native")]
thread_local! {
    static SERVERS: RefCell<Vec<Option<ServerState>>> = const { RefCell::new(Vec::new()) };
}

// ---- the "proc" module (background processes) ----

thread_local! {
    static PROCS: RefCell<Vec<Option<std::process::Child>>> = const { RefCell::new(Vec::new()) };
}

#[cfg(feature = "native")]
fn db_with<T>(h: i64, f: impl FnOnce(&rusqlite::Connection) -> Result<T, String>) -> Result<T, String> {
    DBS.with(|d| {
        let d = d.borrow();
        match usize::try_from(h).ok().and_then(|i| d.get(i)).and_then(|o| o.as_ref()) {
            Some(conn) => f(conn),
            None => Err("db: this is not an open database (use db.open first)".to_string()),
        }
    })
}

#[cfg(feature = "native")]
fn sqlite_to_string(v: rusqlite::types::ValueRef) -> String {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => String::new(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(r) => format_real(r),
        ValueRef::Text(t) | ValueRef::Blob(t) => String::from_utf8_lossy(t).into_owned(),
    }
}

#[cfg(feature = "native")]
fn sqlite_to_json(v: rusqlite::types::ValueRef) -> Json {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => Json::Null,
        ValueRef::Integer(n) => Json::from(n),
        ValueRef::Real(r) => serde_json::Number::from_f64(r).map(Json::Number).unwrap_or(Json::Null),
        ValueRef::Text(t) | ValueRef::Blob(t) => Json::String(String::from_utf8_lossy(t).into_owned()),
    }
}

fn collatz_peak(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("collatzmax.peak: the number must be positive".to_string());
    }
    let mut m = n as i128;
    let mut peak = m;
    let mut steps = 0;
    while m != 1 {
        m = if m % 2 == 0 { m / 2 } else { 3 * m + 1 };
        peak = peak.max(m);
        steps += 1;
        if steps > 10_000_000 {
            return Err("collatzmax.peak: too many steps".to_string());
        }
    }
    if peak > i64::MAX as i128 {
        return Err(OVERFLOW.to_string());
    }
    Ok(peak as i64)
}

/// Tallies the items, returning a dictionary from each distinct value to its count.
fn count_into(items: impl Iterator<Item = String>) -> Value {
    let mut d = Dict::default();
    for s in items {
        let k = DictKey::Str(s.into());
        let n = d.get(&k).map(int).unwrap_or(0);
        d.set(k, Value::Int(n + 1));
    }
    Value::Dict(Rc::new(RefCell::new(d)))
}

// -------- JSON (the "data" module) (helpers in data.rs) --------
mod data;
use data::*;

// -------- dates and times (the "clock" module) (helpers in clock.rs) --------
mod clock;
use clock::*;

// -------- running commands (the "shell" module) (helpers in shell.rs) --------
mod shell;
use shell::*;

// ---- module "ffi": call C functions from a native shared library (in ffi.rs) ----
#[cfg(feature = "native")]
mod ffi;
#[cfg(feature = "native")]
use ffi::*;

// ---------------------------------------------------------------- helpers for the extra modules

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a as i64
}

fn is_prime(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    if n % 2 == 0 {
        return n == 2;
    }
    let mut d: i64 = 3;
    while d.saturating_mul(d) <= n {
        if n % d == 0 {
            return false;
        }
        d += 2;
    }
    true
}

fn prime_next(n: i64) -> i64 {
    let mut c = if n < 2 { 2 } else { n + 1 };
    while !is_prime(c) {
        c += 1;
    }
    c
}

fn prime_nth(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("prime.nth: the position starts at 1".to_string());
    }
    let (mut count, mut c) = (0, 1);
    while count < n {
        c += 1;
        if is_prime(c) {
            count += 1;
        }
    }
    Ok(c)
}

fn fib_at(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("fib.at: the position cannot be negative".to_string());
    }
    let (mut a, mut b): (i64, i64) = (0, 1);
    for _ in 0..n {
        let next = a.checked_add(b).ok_or(OVERFLOW)?;
        (a, b) = (b, next);
    }
    Ok(a)
}

fn int_to_base(n: i64, base: i64) -> Result<String, String> {
    if !(2..=36).contains(&base) {
        return Err("base.to: the base must be from 2 to 36".to_string());
    }
    if n == 0 {
        return Ok("0".to_string());
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let (base, mut v) = (base as u128, (n as i128).unsigned_abs());
    let mut out = Vec::new();
    while v > 0 {
        out.push(digits[(v % base) as usize]);
        v /= base;
    }
    if n < 0 {
        out.push(b'-');
    }
    out.reverse();
    Ok(String::from_utf8(out).unwrap())
}

fn base_from(s: &str, base: i64) -> Result<i64, String> {
    if !(2..=36).contains(&base) {
        return Err("base.from: the base must be from 2 to 36".to_string());
    }
    let t = s.trim();
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let n = i64::from_str_radix(body, base as u32)
        .map_err(|_| format!("base.from: \"{}\" is not a base-{} number", s, base))?;
    Ok(if neg { -n } else { n })
}

const ROMAN: &[(i64, &str)] = &[
    (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"),
    (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
];

fn roman_to(n: i64) -> Result<String, String> {
    if !(1..=3999).contains(&n) {
        return Err("roman.to: the number must be from 1 to 3999".to_string());
    }
    let mut n = n;
    let mut out = String::new();
    for &(v, sym) in ROMAN {
        while n >= v {
            out.push_str(sym);
            n -= v;
        }
    }
    Ok(out)
}

fn roman_from(s: &str) -> Result<i64, String> {
    let up = s.trim().to_uppercase();
    let val = |c: char| match c {
        'I' => 1, 'V' => 5, 'X' => 10, 'L' => 50, 'C' => 100, 'D' => 500, 'M' => 1000, _ => 0,
    };
    if up.is_empty() || up.chars().any(|c| val(c) == 0) {
        return Err(format!("roman.from: \"{}\" is not a Roman numeral", s));
    }
    let nums: Vec<i64> = up.chars().map(val).collect();
    let mut total = 0;
    for i in 0..nums.len() {
        if i + 1 < nums.len() && nums[i] < nums[i + 1] {
            total -= nums[i];
        } else {
            total += nums[i];
        }
    }
    // a valid numeral is the one roman_to would produce
    if roman_to(total).map(|r| r == up).unwrap_or(false) {
        Ok(total)
    } else {
        Err(format!("roman.from: \"{}\" is not a valid Roman numeral", s))
    }
}

fn size_human(n: i64) -> String {
    if n < 0 {
        return format!("-{}", size_human(-n));
    }
    let units = ["B", "KB", "MB", "GB", "TB", "PB"];
    let (mut v, mut i) = (n as f64, 0);
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} B", n)
    } else {
        format!("{:.1} {}", v, units[i])
    }
}

fn duration_human(total: i64) -> String {
    if total < 0 {
        return format!("-{}", duration_human(-total));
    }
    if total == 0 {
        return "0s".to_string();
    }
    let parts = [(86400, "d"), (3600, "h"), (60, "m"), (1, "s")];
    let mut rest = total;
    let mut out = Vec::new();
    for (size, name) in parts {
        let n = rest / size;
        if n > 0 {
            out.push(format!("{}{}", n, name));
            rest %= size;
        }
    }
    out.join(" ")
}

fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // mean Earth radius, km
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dphi = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dphi / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn compass_direction(deg: f64) -> String {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let i = ((deg.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8;
    POINTS[i].to_string()
}

fn ordinal_of(n: i64) -> String {
    let last_two = n.unsigned_abs() % 100;
    let suffix = if (11..=13).contains(&last_two) {
        "th"
    } else {
        match n.unsigned_abs() % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{}{}", n, suffix)
}

fn pad(s: &str, width: i64, fill: &str, left: bool) -> String {
    let fc = fill.chars().next().unwrap_or(' ');
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let padding: String = std::iter::repeat(fc).take((width - len) as usize).collect();
    if left {
        format!("{}{}", padding, s)
    } else {
        format!("{}{}", s, padding)
    }
}

fn center(s: &str, width: i64, fill: &str) -> String {
    let fc = fill.chars().next().unwrap_or(' ');
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let total = (width - len) as usize;
    let left = total / 2;
    let fillup = |n: usize| std::iter::repeat(fc).take(n).collect::<String>();
    format!("{}{}{}", fillup(left), s, fillup(total - left))
}

fn hex_decode(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err("hex.decode: the text must have an even number of digits".to_string());
    }
    let b = s.as_bytes();
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for pair in b.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).ok_or("hex.decode: not a hex digit")?;
        let lo = (pair[1] as char).to_digit(16).ok_or("hex.decode: not a hex digit")?;
        bytes.push((hi * 16 + lo) as u8);
    }
    String::from_utf8(bytes).map_err(|_| "hex.decode: the data is not text".to_string())
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn url_decode(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                if i + 2 >= b.len() {
                    return Err("url.decode: a % is not followed by two hex digits".to_string());
                }
                let hi = (b[i + 1] as char).to_digit(16).ok_or("url.decode: bad hex after %")?;
                let lo = (b[i + 2] as char).to_digit(16).ok_or("url.decode: bad hex after %")?;
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "url.decode: the data is not text".to_string())
}

fn case_title(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Splits text into words, breaking on non-alphanumerics and at lowercase→uppercase.
fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && prev_lower && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
            prev_lower = c.is_lowercase() || c.is_numeric();
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn case_join(s: &str, sep: char) -> String {
    words(s).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join(&sep.to_string())
}

fn case_camel(s: &str) -> String {
    words(s)
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let low = w.to_lowercase();
            if i == 0 {
                low
            } else {
                let mut ch = low.chars();
                match ch.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect()
}

fn slug_make(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn all_chars(s: &str, pred: impl Fn(char) -> bool) -> bool {
    !s.is_empty() && s.chars().all(pred)
}

fn all_cased(s: &str, upper: bool) -> bool {
    let mut any = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            any = true;
            if upper != c.is_uppercase() {
                return false;
            }
        }
    }
    any
}

fn caesar(s: &str, shift: i64) -> String {
    let sh = shift.rem_euclid(26) as u8;
    s.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                (b'A' + (c as u8 - b'A' + sh) % 26) as char
            } else if c.is_ascii_lowercase() {
                (b'a' + (c as u8 - b'a' + sh) % 26) as char
            } else {
                c
            }
        })
        .collect()
}

fn leet_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c.to_ascii_lowercase() {
            'a' => '4',
            'e' => '3',
            'i' => '1',
            'o' => '0',
            's' => '5',
            't' => '7',
            _ => c,
        })
        .collect()
}

const MORSE: &[(char, &str)] = &[
    ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."), ('F', "..-."), ('G', "--."), ('H', "...."),
    ('I', ".."), ('J', ".---"), ('K', "-.-"), ('L', ".-.."), ('M', "--"), ('N', "-."), ('O', "---"), ('P', ".--."),
    ('Q', "--.-"), ('R', ".-."), ('S', "..."), ('T', "-"), ('U', "..-"), ('V', "...-"), ('W', ".--"), ('X', "-..-"),
    ('Y', "-.--"), ('Z', "--.."), ('0', "-----"), ('1', ".----"), ('2', "..---"), ('3', "...--"), ('4', "....-"),
    ('5', "....."), ('6', "-...."), ('7', "--..."), ('8', "---.."), ('9', "----."),
];

fn morse_encode(s: &str) -> Result<String, String> {
    let mut out = Vec::new();
    for c in s.to_uppercase().chars() {
        if c.is_whitespace() {
            out.push("/".to_string());
        } else if let Some((_, code)) = MORSE.iter().find(|(ch, _)| *ch == c) {
            out.push(code.to_string());
        } else {
            return Err(format!("morse.encode: cannot encode '{}'", c));
        }
    }
    Ok(out.join(" "))
}

fn morse_decode(s: &str) -> Result<String, String> {
    let mut out = String::new();
    for token in s.split_whitespace() {
        if token == "/" {
            out.push(' ');
        } else if let Some((ch, _)) = MORSE.iter().find(|(_, code)| *code == token) {
            out.push(*ch);
        } else {
            return Err(format!("morse.decode: unknown code \"{}\"", token));
        }
    }
    Ok(out)
}

const NATO: &[(char, &str)] = &[
    ('A', "Alpha"), ('B', "Bravo"), ('C', "Charlie"), ('D', "Delta"), ('E', "Echo"), ('F', "Foxtrot"), ('G', "Golf"),
    ('H', "Hotel"), ('I', "India"), ('J', "Juliett"), ('K', "Kilo"), ('L', "Lima"), ('M', "Mike"), ('N', "November"),
    ('O', "Oscar"), ('P', "Papa"), ('Q', "Quebec"), ('R', "Romeo"), ('S', "Sierra"), ('T', "Tango"), ('U', "Uniform"),
    ('V', "Victor"), ('W', "Whiskey"), ('X', "X-ray"), ('Y', "Yankee"), ('Z', "Zulu"), ('0', "Zero"), ('1', "One"),
    ('2', "Two"), ('3', "Three"), ('4', "Four"), ('5', "Five"), ('6', "Six"), ('7', "Seven"), ('8', "Eight"),
    ('9', "Nine"),
];

fn nato_encode(s: &str) -> String {
    s.to_uppercase()
        .chars()
        .filter_map(|c| NATO.iter().find(|(ch, _)| *ch == c).map(|(_, w)| (*w).to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn color_invert(s: &str) -> Result<String, String> {
    let t = s.trim();
    let h = t.strip_prefix('#').unwrap_or(t);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("color.invert: \"{}\" is not a #rrggbb color", s));
    }
    let n = u32::from_str_radix(h, 16).unwrap();
    Ok(format!("#{:06x}", 0xFF_FFFF - n))
}

fn regex_is(pattern: &str, s: &str) -> bool {
    regex::Regex::new(pattern).map(|re| re.is_match(s)).unwrap_or(false)
}

fn luhn_valid(s: &str) -> bool {
    let clean: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut sum = 0;
    for (i, c) in clean.chars().rev().enumerate() {
        let mut d = c.to_digit(10).unwrap();
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum % 10 == 0
}

fn isbn_valid(s: &str) -> bool {
    let clean: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    match clean.len() {
        10 => {
            let mut sum = 0i32;
            for (i, c) in clean.chars().enumerate() {
                let v = if i == 9 && (c == 'X' || c == 'x') {
                    10
                } else {
                    match c.to_digit(10) {
                        Some(d) => d as i32,
                        None => return false,
                    }
                };
                sum += (10 - i as i32) * v;
            }
            sum % 11 == 0
        }
        13 => {
            let mut sum = 0i32;
            for (i, c) in clean.chars().enumerate() {
                let d = match c.to_digit(10) {
                    Some(d) => d as i32,
                    None => return false,
                };
                sum += if i % 2 == 0 { d } else { 3 * d };
            }
            sum % 10 == 0
        }
        _ => false,
    }
}

enum PathPart {
    Name,
    Stem,
    Ext,
    Parent,
}

fn path_part(p: &str, part: PathPart) -> String {
    let path = std::path::Path::new(p);
    let piece = match part {
        PathPart::Name => path.file_name(),
        PathPart::Stem => path.file_stem(),
        PathPart::Ext => path.extension(),
        PathPart::Parent => return path.parent().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    piece.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The arguments the program was given, after the interpreter and the script path.
fn program_args() -> Vec<String> {
    std::env::args().skip(2).collect()
}

fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => out.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            }
        }
    }
    out.push(cur);
    out
}

fn csv_line(fields: impl Iterator<Item = String>) -> String {
    fields
        .map(|f| {
            if f.contains([',', '"', '\n']) {
                format!("\"{}\"", f.replace('"', "\"\""))
            } else {
                f
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The numbers of an array or list argument (for the `stats` module).
fn numbers(v: &Value) -> Result<Vec<f64>, String> {
    let items = seq(v).borrow();
    if items.is_empty() {
        return Err("the list is empty".to_string());
    }
    no_empty_elements(&items, "stats")?;
    Ok(items.iter().map(real).collect())
}

fn permutations(n: i64, k: i64) -> Result<i64, String> {
    if n < 0 || k < 0 {
        return Err("choose: the values cannot be negative".to_string());
    }
    if k > n {
        return Ok(0);
    }
    let mut acc: i64 = 1;
    for i in 0..k {
        acc = acc.checked_mul(n - i).ok_or(OVERFLOW)?;
    }
    Ok(acc)
}

fn combinations(n: i64, k: i64) -> Result<i64, String> {
    if n < 0 || k < 0 {
        return Err("choose: the values cannot be negative".to_string());
    }
    if k > n {
        return Ok(0);
    }
    let k = k.min(n - k);
    let mut acc: i64 = 1;
    for i in 0..k {
        acc = acc.checked_mul(n - i).ok_or(OVERFLOW)? / (i + 1);
    }
    Ok(acc)
}

fn catalan(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err("seq.catalan: the position cannot be negative".to_string());
    }
    Ok(combinations(2 * n, n)? / (n + 1))
}

fn mod_pow(base: i64, exp: i64, m: i64) -> Result<i64, String> {
    if m <= 0 {
        return Err("modular.pow: the modulus must be positive".to_string());
    }
    if exp < 0 {
        return Err("modular.pow: the exponent cannot be negative".to_string());
    }
    let m = m as i128;
    let mut result: i128 = 1 % m;
    let mut b = (base as i128).rem_euclid(m);
    let mut e = exp as u64;
    while e > 0 {
        if e & 1 == 1 {
            result = result * b % m;
        }
        b = b * b % m;
        e >>= 1;
    }
    Ok(result as i64)
}

fn collatz(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("collatz.steps: the number must be positive".to_string());
    }
    let mut n = n as i128;
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
        if steps > 10_000_000 {
            return Err("collatz.steps: too many steps".to_string());
        }
    }
    Ok(steps)
}

fn is_perfect(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    let mut sum = 1i64;
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            sum += d;
            if d != n / d {
                sum += n / d;
            }
        }
        d += 1;
    }
    sum == n
}

fn is_armstrong(n: i64) -> bool {
    if n < 0 {
        return false;
    }
    let s = n.to_string();
    let power = s.len() as u32;
    let sum: u128 = s.bytes().map(|b| ((b - b'0') as u128).pow(power)).sum();
    sum == n as u128
}

fn mask_middle(s: &str, keep: i64) -> String {
    let chars: Vec<char> = s.chars().collect();
    let k = keep.max(0) as usize;
    if chars.len() <= 2 * k {
        return "*".repeat(chars.len());
    }
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| if i < k || i >= chars.len() - k { *c } else { '*' })
        .collect()
}

fn pig_latin(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let lower = w.to_lowercase();
            let chars: Vec<char> = lower.chars().collect();
            if chars.is_empty() {
                return String::new();
            }
            if "aeiou".contains(chars[0]) {
                format!("{}way", lower)
            } else {
                let idx = chars.iter().position(|c| "aeiou".contains(*c)).unwrap_or(chars.len());
                let head: String = chars[..idx].iter().collect();
                let tail: String = chars[idx..].iter().collect();
                format!("{}{}ay", tail, head)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn caesar_char(c: char, shift: i64) -> char {
    let sh = shift.rem_euclid(26) as u8;
    if c.is_ascii_uppercase() {
        (b'A' + (c as u8 - b'A' + sh) % 26) as char
    } else if c.is_ascii_lowercase() {
        (b'a' + (c as u8 - b'a' + sh) % 26) as char
    } else {
        c
    }
}

fn atbash(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                (b'z' - (c as u8 - b'a')) as char
            } else if c.is_ascii_uppercase() {
                (b'Z' - (c as u8 - b'A')) as char
            } else {
                c
            }
        })
        .collect()
}

fn vigenere(s: &str, key: &str, encode: bool) -> Result<String, String> {
    let k: Vec<i64> = key.chars().filter(|c| c.is_ascii_alphabetic()).map(|c| (c.to_ascii_lowercase() as u8 - b'a') as i64).collect();
    if k.is_empty() {
        return Err("vigenere: the key must contain letters".to_string());
    }
    let mut ki = 0;
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            let shift = if encode { k[ki % k.len()] } else { -k[ki % k.len()] };
            out.push(caesar_char(c, shift));
            ki += 1;
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

fn rot47(s: &str) -> String {
    s.chars()
        .map(|c| {
            let b = c as u32;
            if (33..=126).contains(&b) {
                char::from_u32(33 + (b - 33 + 47) % 94).unwrap()
            } else {
                c
            }
        })
        .collect()
}

fn bin_decode(s: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    for group in s.split_whitespace() {
        bytes.push(u8::from_str_radix(group, 2).map_err(|_| format!("bin.decode: \"{}\" is not 8 bits", group))?);
    }
    String::from_utf8(bytes).map_err(|_| "bin.decode: the data is not text".to_string())
}

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(5) {
        let mut buf = [0u8; 5];
        buf[..chunk.len()].copy_from_slice(chunk);
        let n = (u64::from(buf[0]) << 32)
            | (u64::from(buf[1]) << 24)
            | (u64::from(buf[2]) << 16)
            | (u64::from(buf[3]) << 8)
            | u64::from(buf[4]);
        let keep = match chunk.len() {
            1 => 2,
            2 => 4,
            3 => 5,
            4 => 7,
            _ => 8,
        };
        for i in 0..8 {
            if i < keep {
                out.push(B32[((n >> (35 - i * 5)) & 31) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base32_decode(s: &str) -> Result<String, String> {
    let mut bits = 0u64;
    let mut nbits = 0u32;
    let mut out = Vec::new();
    for ch in s.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=') {
        let up = ch.to_ascii_uppercase();
        let val = B32.iter().position(|&x| x == up).ok_or_else(|| format!("base32.decode: '{}' is not a base32 character", ch as char))? as u64;
        bits = (bits << 5) | val;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    String::from_utf8(out).map_err(|_| "base32.decode: the data is not text".to_string())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn html_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

fn anagram(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let mut v: Vec<char> = s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
        v.sort_unstable();
        v
    };
    !a.is_empty() && norm(a) == norm(b)
}

fn rle_encode(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let mut count = 1;
        while chars.peek() == Some(&c) {
            chars.next();
            count += 1;
        }
        out.push(c);
        out.push_str(&count.to_string());
    }
    out
}

fn rle_decode(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let mut num = String::new();
        while let Some(d) = chars.peek() {
            if d.is_ascii_digit() {
                num.push(*d);
                chars.next();
            } else {
                break;
            }
        }
        if num.is_empty() {
            return Err("rle.decode: a character is not followed by a count".to_string());
        }
        let count: usize = num.parse().map_err(|_| "rle.decode: the count is too large".to_string())?;
        if count > 10_000_000 {
            return Err("rle.decode: the count is too large".to_string());
        }
        for _ in 0..count {
            out.push(c);
        }
    }
    Ok(out)
}

fn wrap_at(s: &str, width: i64) -> String {
    let w = width.max(1) as usize;
    let mut lines = Vec::new();
    for para in s.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if line.is_empty() {
                line = word.to_string();
            } else if line.chars().count() + 1 + word.chars().count() <= w {
                line.push(' ');
                line.push_str(word);
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn spongebob(s: &str) -> String {
    let mut upper = false;
    s.chars()
        .map(|c| {
            if c.is_alphabetic() {
                upper = !upper;
                if upper {
                    c.to_ascii_uppercase()
                } else {
                    c.to_ascii_lowercase()
                }
            } else {
                c
            }
        })
        .collect()
}

fn boolv(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => panic!("internal error: expected a bool, found {:?}", other),
    }
}

fn bmi_category(b: f64) -> String {
    (if b < 18.5 {
        "underweight"
    } else if b < 25.0 {
        "normal"
    } else if b < 30.0 {
        "overweight"
    } else {
        "obese"
    })
    .to_string()
}

fn planet_gravity(name: &str) -> Result<f64, String> {
    Ok(match name.to_lowercase().as_str() {
        "mercury" => 3.7,
        "venus" => 8.87,
        "earth" => 9.81,
        "moon" => 1.62,
        "mars" => 3.71,
        "jupiter" => 24.79,
        "saturn" => 10.44,
        "uranus" => 8.69,
        "neptune" => 11.15,
        "pluto" => 0.62,
        "sun" => 274.0,
        _ => return Err(format!("gravity.on: unknown place \"{}\"", name)),
    })
}

fn loan_payment(principal: f64, annual_rate: f64, months: i64) -> Result<f64, String> {
    if months <= 0 {
        return Err("loan.payment: the number of months must be positive".to_string());
    }
    let r = annual_rate / 100.0 / 12.0;
    let n = months as f64;
    if r == 0.0 {
        return Ok(principal / n);
    }
    Ok(principal * r * (1.0 + r).powf(n) / ((1.0 + r).powf(n) - 1.0))
}

fn money_format(amount: f64) -> String {
    let neg = amount < 0.0;
    let s = format!("{:.2}", amount.abs());
    let (int_part, dec) = s.split_once('.').unwrap_or((s.as_str(), "00"));
    let bytes = int_part.as_bytes();
    let mut grouped = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*b as char);
    }
    format!("{}{}.{}", if neg { "-" } else { "" }, grouped, dec)
}

fn bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

fn heron(a: f64, b: f64, c: f64) -> Result<f64, String> {
    if a <= 0.0 || b <= 0.0 || c <= 0.0 || a + b <= c || a + c <= b || b + c <= a {
        return Err("triangle.heron: these sides do not form a triangle".to_string());
    }
    let s = (a + b + c) / 2.0;
    Ok((s * (s - a) * (s - b) * (s - c)).sqrt())
}

fn weekday_of(stamp: i64) -> Result<String, String> {
    use chrono::Datelike;
    let dt = chrono::DateTime::from_timestamp(stamp, 0).ok_or("weekday.of: the timestamp is out of range")?;
    const NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    Ok(NAMES[dt.weekday().num_days_from_monday() as usize].to_string())
}

fn month_name(n: i64) -> Result<String, String> {
    const M: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
        "December",
    ];
    if !(1..=12).contains(&n) {
        return Err(format!("month.name: {} is not a month (1-12)", n));
    }
    Ok(M[(n - 1) as usize].to_string())
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> Result<i64, String> {
    Ok(match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => return Err(format!("month.days: {} is not a month (1-12)", m)),
    })
}

fn age_years(birth: i64, now: i64) -> i64 {
    use chrono::Datelike;
    match (chrono::DateTime::from_timestamp(birth, 0), chrono::DateTime::from_timestamp(now, 0)) {
        (Some(b), Some(n)) => {
            let mut years = n.year() - b.year();
            if (n.month(), n.day()) < (b.month(), b.day()) {
                years -= 1;
            }
            years as i64
        }
        _ => 0,
    }
}

fn zodiac_sign(month: i64, day: i64) -> Result<String, String> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err("zodiac.sign: that is not a valid date".to_string());
    }
    let sign = match month {
        1 => if day < 20 { "Capricorn" } else { "Aquarius" },
        2 => if day < 19 { "Aquarius" } else { "Pisces" },
        3 => if day < 21 { "Pisces" } else { "Aries" },
        4 => if day < 20 { "Aries" } else { "Taurus" },
        5 => if day < 21 { "Taurus" } else { "Gemini" },
        6 => if day < 21 { "Gemini" } else { "Cancer" },
        7 => if day < 23 { "Cancer" } else { "Leo" },
        8 => if day < 23 { "Leo" } else { "Virgo" },
        9 => if day < 23 { "Virgo" } else { "Libra" },
        10 => if day < 23 { "Libra" } else { "Scorpio" },
        11 => if day < 22 { "Scorpio" } else { "Sagittarius" },
        _ => if day < 22 { "Sagittarius" } else { "Capricorn" },
    };
    Ok(sign.to_string())
}

fn season_of(m: i64) -> Result<String, String> {
    Ok(match m {
        12 | 1 | 2 => "winter",
        3 | 4 | 5 => "spring",
        6 | 7 | 8 => "summer",
        9 | 10 | 11 => "autumn",
        _ => return Err(format!("season.of: {} is not a month", m)),
    }
    .to_string())
}

fn grade_letter(score: f64) -> String {
    (if score >= 90.0 {
        "A"
    } else if score >= 80.0 {
        "B"
    } else if score >= 70.0 {
        "C"
    } else if score >= 60.0 {
        "D"
    } else {
        "F"
    })
    .to_string()
}

fn fraction_simplify(n: i64, d: i64) -> Result<String, String> {
    if d == 0 {
        return Err("fraction.simplify: the denominator is zero".to_string());
    }
    let g = gcd(n, d).max(1);
    let (mut nn, mut dd) = (n / g, d / g);
    if dd < 0 {
        nn = -nn;
        dd = -dd;
    }
    Ok(format!("{}/{}", nn, dd))
}

fn fnv1a(s: &str) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h as i64
}

fn djb2(s: &str) -> i64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h as i64
}

fn sdbm(s: &str) -> i64 {
    let mut h: u64 = 0;
    for b in s.bytes() {
        h = (b as u64).wrapping_add(h.wrapping_shl(6)).wrapping_add(h.wrapping_shl(16)).wrapping_sub(h);
    }
    h as i64
}

fn adler32(s: &str) -> i64 {
    let (mut a, mut b): (u32, u32) = (1, 0);
    for byte in s.bytes() {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (((b << 16) | a) as u64) as i64
}

fn ipv4_valid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()) && p.parse::<u32>().map(|n| n <= 255).unwrap_or(false)
        })
}

thread_local! {
    static RNG: std::cell::Cell<u64> = std::cell::Cell::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64)
            | 1,
    );
}

fn rng_u64() -> u64 {
    RNG.with(|c| {
        let mut x = c.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        c.set(x);
        x
    })
}

fn rng_range(lo: i64, hi: i64) -> i64 {
    if lo >= hi {
        return lo;
    }
    let span = (hi as i128 - lo as i128 + 1) as u128;
    (lo as i128 + (rng_u64() as u128 % span) as i128) as i64
}

fn rng_hex(len: i64) -> Result<String, String> {
    if len < 0 {
        return Err("token.hex: the length cannot be negative".to_string());
    }
    if len > 100_000 {
        return Err("token.hex: the length is too large".to_string());
    }
    let digits = b"0123456789abcdef";
    Ok((0..len).map(|_| digits[(rng_u64() % 16) as usize] as char).collect())
}

fn rng_password(len: i64) -> Result<String, String> {
    if len < 0 {
        return Err("password.make: the length cannot be negative".to_string());
    }
    if len > 100_000 {
        return Err("password.make: the length is too large".to_string());
    }
    let set = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%&*?";
    Ok((0..len).map(|_| set[(rng_u64() % set.len() as u64) as usize] as char).collect())
}

fn rng_nanoid() -> String {
    let set = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    (0..21).map(|_| set[(rng_u64() % 64) as usize] as char).collect()
}

const LOREM: &[&str] = &[
    "lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do", "eiusmod", "tempor",
    "incididunt", "ut", "labore", "et", "dolore", "magna", "aliqua",
];

fn lorem_words(n: i64) -> String {
    let n = n.max(0) as usize;
    (0..n).map(|i| LOREM[i % LOREM.len()]).collect::<Vec<_>>().join(" ")
}

fn caps_first(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

fn divisor_count(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("factor.count: the number must be positive".to_string());
    }
    let mut count = 0;
    let mut d = 1;
    while d * d <= n {
        if n % d == 0 {
            count += if d == n / d { 1 } else { 2 };
        }
        d += 1;
    }
    Ok(count)
}

fn divisor_sum(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("factor.sum: the number must be positive".to_string());
    }
    let mut sum = 0i64;
    let mut d = 1;
    while d * d <= n {
        if n % d == 0 {
            sum += d;
            if d != n / d {
                sum += n / d;
            }
        }
        d += 1;
    }
    Ok(sum)
}

fn totient(n: i64) -> Result<i64, String> {
    if n < 1 {
        return Err("totient.of: the number must be positive".to_string());
    }
    let mut result = n;
    let mut nn = n;
    let mut p = 2;
    while p * p <= nn {
        if nn % p == 0 {
            while nn % p == 0 {
                nn /= p;
            }
            result -= result / p;
        }
        p += 1;
    }
    if nn > 1 {
        result -= result / nn;
    }
    Ok(result)
}

fn heat_index(tc: f64, rh: f64) -> f64 {
    let t = tc * 9.0 / 5.0 + 32.0;
    let hi = -42.379 + 2.049_015_23 * t + 10.143_331_27 * rh - 0.224_755_41 * t * rh
        - 0.006_837_83 * t * t
        - 0.054_817_17 * rh * rh
        + 0.001_228_74 * t * t * rh
        + 0.000_852_82 * t * rh * rh
        - 0.000_001_99 * t * t * rh * rh;
    (hi - 32.0) * 5.0 / 9.0
}

fn quadratic_root(a: f64, b: f64, c: f64, plus: bool) -> Result<f64, String> {
    if a == 0.0 {
        return Err("quadratic: 'a' is zero, so this is not a quadratic equation".to_string());
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return Err("quadratic: there are no real roots".to_string());
    }
    let sq = disc.sqrt();
    Ok(if plus { (-b + sq) / (2.0 * a) } else { (-b - sq) / (2.0 * a) })
}

// ---------------------------------------------------------------- the "grid" array engine
//
// Grids are contiguous Vec<f64> buffers kept in a registry; a Globw program holds
// an int handle. Element-wise kernels and reductions loop over unboxed memory in
// parallel (rayon), which is where the speed against a scalar loop comes from.

fn floats(v: &Value) -> &Rc<RefCell<Vec<f64>>> {
    match v {
        Value::Floats(g, _) => g,
        other => panic!("internal error: expected a nungw, found {:?}", other),
    }
}

/// The shape (sizes along each dimension) of a nungw value.
fn shape_of(v: &Value) -> Rc<Vec<usize>> {
    match v {
        Value::Floats(_, shape) => shape.clone(),
        #[cfg(feature = "gpu")]
        Value::GpuFloats(_, shape) => shape.clone(),
        other => panic!("internal error: expected a nungw, found {:?}", other),
    }
}

/// Build a 1-D nungw from a flat buffer.
fn grid_val(data: Vec<f64>) -> Value {
    let shape = Rc::new(vec![data.len()]);
    Value::Floats(Rc::new(RefCell::new(data)), shape)
}

/// Build a nungw from a flat buffer with a given shape. The caller guarantees
/// the shape's product equals the buffer length.
fn grid_val_shaped(data: Vec<f64>, shape: Vec<usize>) -> Value {
    Value::Floats(Rc::new(RefCell::new(data)), Rc::new(shape))
}

/// An element-wise map keeps the input's shape.
fn grid_map(v: &Value, f: impl Fn(f64) -> f64 + Sync + Send) -> Value {
    let data: Vec<f64> = floats(v).borrow().par_iter().map(|&x| f(x)).collect();
    grid_val_shaped(data, (*shape_of(v)).clone())
}

fn grid_zip(a: &Value, b: &Value, f: impl Fn(f64, f64) -> f64 + Sync + Send) -> Result<Value, String> {
    let (sa, sb) = (shape_of(a), shape_of(b));
    // Equal shapes: element-wise, keeping the shape. Differing shapes:
    // broadcast (NumPy rules).
    if *sa == *sb {
        let (va, vb) = (floats(a).borrow(), floats(b).borrow());
        let data: Vec<f64> = va.par_iter().zip(vb.par_iter()).map(|(&x, &y)| f(x, y)).collect();
        return Ok(grid_val_shaped(data, (*sa).clone()));
    }
    broadcast_zip(a, b, &sa, &sb, f)
}

/// The broadcast of two shapes (NumPy rules: align from the right, each pair of
/// sizes must be equal or one of them 1), or an error if they do not broadcast.
fn broadcast_shape(sa: &[usize], sb: &[usize]) -> Result<Vec<usize>, String> {
    let n = sa.len().max(sb.len());
    let mut out = vec![0usize; n];
    for i in 0..n {
        let da = if i < n - sa.len() { 1 } else { sa[i - (n - sa.len())] };
        let db = if i < n - sb.len() { 1 } else { sb[i - (n - sb.len())] };
        out[i] = if da == db || db == 1 {
            da
        } else if da == 1 {
            db
        } else {
            return Err(format!("nungw: shapes do not broadcast ({sa:?} and {sb:?})"));
        };
    }
    Ok(out)
}

/// The strides (in elements) of a row-major array of this shape, as a value
/// moving by one along each dimension; a dimension of size 1 gets stride 0 so
/// it repeats (broadcasting).
fn broadcast_strides(shape: &[usize], full: &[usize]) -> Vec<usize> {
    // row-major strides of `shape`, then mapped onto `full` (right-aligned)
    let mut stride = vec![0usize; shape.len()];
    let mut acc = 1usize;
    for i in (0..shape.len()).rev() {
        stride[i] = if shape[i] == 1 { 0 } else { acc };
        acc *= shape[i];
    }
    let mut out = vec![0usize; full.len()];
    let off = full.len() - shape.len();
    for i in 0..shape.len() {
        out[off + i] = stride[i];
    }
    out
}

fn broadcast_zip(
    a: &Value,
    b: &Value,
    sa: &[usize],
    sb: &[usize],
    f: impl Fn(f64, f64) -> f64 + Sync + Send,
) -> Result<Value, String> {
    let out_shape = broadcast_shape(sa, sb)?;
    let total: usize = out_shape.iter().product();
    let stride_a = broadcast_strides(sa, &out_shape);
    let stride_b = broadcast_strides(sb, &out_shape);
    let (va, vb) = (floats(a).borrow(), floats(b).borrow());
    let n = out_shape.len();
    let mut out = vec![0.0; total];
    // iterate over the multi-index; compute flat offsets into a and b
    let mut idx = vec![0usize; n];
    for slot in out.iter_mut() {
        let mut oa = 0usize;
        let mut ob = 0usize;
        for d in 0..n {
            oa += idx[d] * stride_a[d];
            ob += idx[d] * stride_b[d];
        }
        *slot = f(va[oa], vb[ob]);
        // increment the multi-index (row-major)
        for d in (0..n).rev() {
            idx[d] += 1;
            if idx[d] < out_shape[d] {
                break;
            }
            idx[d] = 0;
        }
    }
    Ok(grid_val_shaped(out, out_shape))
}

/// LU decomposition with partial pivoting, in place on `m` (an n x n row-major
/// matrix). On return `m` holds L (below the diagonal, unit diagonal implied)
/// and U (on and above), `perm` the row permutation. Gives the permutation sign
/// (for the determinant), or None if the matrix is singular. Pure Rust so it
/// works on wasm too (no LAPACK).
fn lu_in_place(m: &mut [f64], n: usize, perm: &mut [usize]) -> Option<f64> {
    for (i, slot) in perm.iter_mut().enumerate() {
        *slot = i;
    }
    let mut sign = 1.0;
    for k in 0..n {
        let mut p = k;
        let mut best = m[k * n + k].abs();
        for i in (k + 1)..n {
            let v = m[i * n + k].abs();
            if v > best {
                best = v;
                p = i;
            }
        }
        if best == 0.0 {
            return None;
        }
        if p != k {
            for j in 0..n {
                m.swap(k * n + j, p * n + j);
            }
            perm.swap(k, p);
            sign = -sign;
        }
        let pivot = m[k * n + k];
        for i in (k + 1)..n {
            let factor = m[i * n + k] / pivot;
            m[i * n + k] = factor;
            for j in (k + 1)..n {
                m[i * n + j] -= factor * m[k * n + j];
            }
        }
    }
    Some(sign)
}

/// Solve A x = b given A's LU decomposition (from `lu_in_place`) and the row
/// permutation, by forward then back substitution.
fn lu_solve(lu: &[f64], n: usize, perm: &[usize], b: &[f64]) -> Vec<f64> {
    let mut x: Vec<f64> = (0..n).map(|i| b[perm[i]]).collect();
    for i in 0..n {
        for j in 0..i {
            x[i] -= lu[i * n + j] * x[j];
        }
    }
    for i in (0..n).rev() {
        for j in (i + 1)..n {
            x[i] -= lu[i * n + j] * x[j];
        }
        x[i] /= lu[i * n + i];
    }
    x
}

/// Reduce an N-d array along one axis (0-based). `combine` folds the values,
/// `finish` post-processes each cell with the count (for mean). Returns a new
/// nungw whose shape has that axis removed (`[1]` if nothing is left).
fn axis_reduce(
    v: &Value,
    axis: i64,
    init: f64,
    combine: impl Fn(f64, f64) -> f64,
    finish: impl Fn(f64, usize) -> f64,
) -> Result<Value, String> {
    let shape = shape_of(v);
    let ndim = shape.len();
    if axis < 0 || axis as usize >= ndim {
        return Err(format!("nungw: axis {axis} is out of range for a {ndim}-D nungw"));
    }
    let axis = axis as usize;
    let data = floats(v).borrow();
    let outer: usize = shape[..axis].iter().product();
    let mid = shape[axis];
    let inner: usize = shape[axis + 1..].iter().product();
    let mut out = vec![init; outer * inner];
    for o in 0..outer {
        for k in 0..mid {
            for i in 0..inner {
                let dst = o * inner + i;
                out[dst] = combine(out[dst], data[(o * mid + k) * inner + i]);
            }
        }
    }
    for cell in out.iter_mut() {
        *cell = finish(*cell, mid);
    }
    let mut out_shape: Vec<usize> = shape[..axis].to_vec();
    out_shape.extend_from_slice(&shape[axis + 1..]);
    if out_shape.is_empty() {
        out_shape.push(1);
    }
    Ok(grid_val_shaped(out, out_shape))
}

/// Eigenvalues of a symmetric n x n matrix, by the classic cyclic Jacobi
/// rotations. Pure Rust (works on wasm). The input is assumed symmetric; the
/// returned values are unsorted (callers sort).
/// Jacobi eigen-decomposition of a symmetric n x n matrix (row-major): the
/// eigenvalues, and the eigenvector matrix V (row-major, n x n) whose column j is
/// the unit eigenvector for eigenvalue j, in Jacobi's natural (unsorted) order.
fn jacobi_eigen(mat: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut a = mat.to_vec();
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    for _sweep in 0..100 {
        let mut off = 0.0;
        for p in 0..n {
            for q in (p + 1)..n {
                off += a[p * n + q] * a[p * n + q];
            }
        }
        if off <= 1e-28 {
            break;
        }
        for p in 0..n {
            for q in (p + 1)..n {
                let apq = a[p * n + q];
                if apq.abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q * n + q] - a[p * n + p]) / (2.0 * apq);
                let sgn = if theta >= 0.0 { 1.0 } else { -1.0 };
                let t = sgn / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                // A <- J^T A J : rotate columns p,q then rows p,q
                for i in 0..n {
                    let aip = a[i * n + p];
                    let aiq = a[i * n + q];
                    a[i * n + p] = c * aip - s * aiq;
                    a[i * n + q] = s * aip + c * aiq;
                }
                for i in 0..n {
                    let api = a[p * n + i];
                    let aqi = a[q * n + i];
                    a[p * n + i] = c * api - s * aqi;
                    a[q * n + i] = s * api + c * aqi;
                }
                // accumulate the rotation into V (V <- V J) for the eigenvectors
                for i in 0..n {
                    let vip = v[i * n + p];
                    let viq = v[i * n + q];
                    v[i * n + p] = c * vip - s * viq;
                    v[i * n + q] = s * vip + c * viq;
                }
            }
        }
    }
    let vals = (0..n).map(|i| a[i * n + i]).collect();
    (vals, v)
}

fn jacobi_eigenvalues(mat: &[f64], n: usize) -> Vec<f64> {
    jacobi_eigen(mat, n).0
}

/// Thin SVD of an m x n matrix A (row-major) via the eigen-decomposition of
/// A^T A: returns the r = min(m,n) singular values (descending), U (m x r) and
/// V (n x r), with the columns of U and V the left/right singular vectors. So
/// A ~= U * diag(sigma) * V^T.
fn svd_thin(a: &[f64], m: usize, n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    // A^T A (n x n, symmetric)
    let mut ata = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0;
            for k in 0..m {
                s += a[k * n + i] * a[k * n + j];
            }
            ata[i * n + j] = s;
        }
    }
    let (eigvals, vecs) = jacobi_eigen(&ata, n);
    // order by eigenvalue (= sigma^2) descending
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&i, &j| eigvals[j].partial_cmp(&eigvals[i]).unwrap_or(std::cmp::Ordering::Equal));
    let r = m.min(n);
    let mut sig = vec![0.0; r];
    let mut vmat = vec![0.0; n * r]; // V: n x r
    for c in 0..r {
        let oc = idx[c];
        sig[c] = eigvals[oc].max(0.0).sqrt();
        for row in 0..n {
            vmat[row * r + c] = vecs[row * n + oc];
        }
    }
    // U columns: u_c = A v_c / sigma_c (left zero when sigma ~ 0)
    let mut umat = vec![0.0; m * r]; // U: m x r
    for c in 0..r {
        if sig[c] > 1e-12 {
            for row in 0..m {
                let mut acc = 0.0;
                for k in 0..n {
                    acc += a[row * n + k] * vmat[k * r + c];
                }
                umat[row * r + c] = acc / sig[c];
            }
        }
    }
    (sig, umat, vmat)
}

/// The optimized BLAS, loaded once at run time if one can be found. Globw uses
/// it for `nungw.matmul` (as NumPy uses OpenBLAS); without it, a parallel pure-Rust
/// kernel is used instead. Looked for via the `GLOBW_BLAS` path, then common names.
#[cfg(feature = "native")]
static BLAS: std::sync::OnceLock<Option<libloading::Library>> = std::sync::OnceLock::new();

/// A row-major C = A*B (ILP64 cblas) as exported by OpenBLAS / scipy-openblas.
#[cfg(feature = "native")]
type CblasDgemm = unsafe extern "C" fn(
    i32, i32, i32, // layout, transA, transB
    i64, i64, i64, // M, N, K
    f64, *const f64, i64, // alpha, A, lda
    *const f64, i64, // B, ldb
    f64, *mut f64, i64, // beta, C, ldc
);

#[cfg(feature = "native")]
fn blas_lib() -> Option<&'static libloading::Library> {
    BLAS.get_or_init(|| {
        let mut names: Vec<String> = Vec::new();
        if let Ok(p) = std::env::var("GLOBW_BLAS") {
            names.push(p);
        }
        names.extend(["openblas.dll", "libopenblas.dll", "libopenblas.so", "libopenblas.dylib"].map(String::from));
        for name in names {
            // SAFETY: loading a shared library; the symbol is used with the matching signature.
            if let Ok(lib) = unsafe { libloading::Library::new(&name) } {
                return Some(lib);
            }
        }
        None
    })
    .as_ref()
}

/// The cblas dgemm entry point, resolved once (a `lib.get`/dlsym per matmul call
/// is pure overhead on the many small products a neural network does). The BLAS
/// library lives in a `'static` OnceLock, so the resolved function pointer stays
/// valid forever.
#[cfg(feature = "native")]
fn blas_dgemm() -> Option<CblasDgemm> {
    static DGEMM: std::sync::OnceLock<Option<CblasDgemm>> = std::sync::OnceLock::new();
    *DGEMM.get_or_init(|| {
        let lib = blas_lib()?;
        // SAFETY: the symbol's ILP64 cblas signature matches CblasDgemm, and the
        // library is never unloaded, so the pointer outlives every use.
        unsafe {
            for sym in [b"scipy_cblas_dgemm64_\0".as_slice(), b"cblas_dgemm64_\0", b"cblas_dgemm_64_\0"] {
                if let Ok(f) = lib.get::<CblasDgemm>(sym) {
                    return Some(*f);
                }
            }
            None
        }
    })
}

/// General C (m x n, row-major) = op(A) * op(B), where op transposes the operand
/// iff its flag is set. A and B are stored row-major; a transposed operand is
/// read through swapped strides / a BLAS trans flag, with no copy. op(A) is
/// m x k, op(B) is k x n. This backs `nungw.matmul_ta`/`matmul_tb`, so a neural
/// network's backward pass (A^T·B, A·B^T) needs no explicit transpose.
fn matmul_trans(out: &mut [f64], a: &[f64], b: &[f64], m: usize, k: usize, n: usize, ta: bool, tb: bool) {
    #[cfg(feature = "native")]
    if let Some(f) = blas_dgemm() {
        // row-major leading dimension = columns of the STORED matrix
        let lda = if ta { m } else { k };
        let ldb = if tb { k } else { n };
        let (tra, trb) = (if ta { 112 } else { 111 }, if tb { 112 } else { 111 });
        // SAFETY: the ILP64 cblas signature matches; lda/ldb describe the stored matrices.
        unsafe {
            f(101, tra, trb, m as i64, n as i64, k as i64, 1.0, a.as_ptr(), lda as i64, b.as_ptr(), ldb as i64, 0.0, out.as_mut_ptr(), n as i64);
        }
        return;
    }
    // matrixmultiply: a transposed operand swaps its (row stride, col stride).
    let (rsa, csa) = if ta { (1isize, m as isize) } else { (k as isize, 1isize) };
    let (rsb, csb) = if tb { (1isize, k as isize) } else { (n as isize, 1isize) };
    // SAFETY: the strides describe the stored row-major matrices of the given lengths.
    unsafe {
        matrixmultiply::dgemm(m, k, n, 1.0, a.as_ptr(), rsa, csa, b.as_ptr(), rsb, csb, 0.0, out.as_mut_ptr(), n as isize, 1);
    }
}

/// C (rows x cols, row-major) = A (rows x inner) * B (inner x cols). Uses the
/// optimized BLAS if present, otherwise a cache-blocked parallel Rust kernel.
pub(crate) fn matmul_into(out: &mut [f64], a: &[f64], b: &[f64], rows: usize, inner: usize, cols: usize) {
    #[cfg(feature = "native")]
    if let Some(f) = blas_dgemm() {
        // 101 = CblasRowMajor, 111 = CblasNoTrans.
        // SAFETY: the ILP64 cblas signature matches; strides describe the row-major matrices.
        unsafe {
            f(101, 111, 111, rows as i64, cols as i64, inner as i64, 1.0, a.as_ptr(), inner as i64, b.as_ptr(), cols as i64, 0.0, out.as_mut_ptr(), cols as i64);
        }
        return;
    }
    // SAFETY: the strides describe row-major matrices of exactly the given lengths.
    unsafe {
        matrixmultiply::dgemm(
            rows, inner, cols, 1.0, a.as_ptr(), inner as isize, 1, b.as_ptr(), cols as isize, 1, 0.0, out.as_mut_ptr(), cols as isize, 1,
        );
    }
}

/// One step of a fused grid kernel (a tiny stack machine run per element).
#[derive(Clone, Copy)]
enum Micro {
    Leaf(usize),
    Scalar(usize),
    Add,
    Sub,
    Mul,
    Div,
    Sqrt,
    Square,
    Abs,
    Un(UnOp),
    Bin(BinOp),
}

/// Binary element-wise ops between two arrays that fuse into the kernel (so
/// `maximum`/`minimum`/`pow`/… compose and, on the GPU, stay resident).
#[derive(Clone, Copy, PartialEq)]
enum BinOp {
    Pow,
    Atan2,
    Hypot,
    Mod,
    Max,
    Min,
}

#[inline]
fn apply_bin(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::Pow => a.powf(b),
        BinOp::Atan2 => a.atan2(b),
        BinOp::Hypot => a.hypot(b),
        BinOp::Mod => a - (a / b).floor() * b,
        BinOp::Max => a.max(b),
        BinOp::Min => a.min(b),
    }
}

/// SIMD form: `Max`/`Min` vectorize; the rest go lane by lane.
#[inline]
fn apply_bin_simd(op: BinOp, a: f64x4, b: f64x4) -> f64x4 {
    match op {
        BinOp::Max => a.max(b),
        BinOp::Min => a.min(b),
        _ => {
            let (aa, bb) = (a.to_array(), b.to_array());
            f64x4::from([
                apply_bin(op, aa[0], bb[0]),
                apply_bin(op, aa[1], bb[1]),
                apply_bin(op, aa[2], bb[2]),
                apply_bin(op, aa[3], bb[3]),
            ])
        }
    }
}

fn bin_token(op: BinOp) -> &'static str {
    match op {
        BinOp::Pow => "pow",
        BinOp::Atan2 => "atan2",
        BinOp::Hypot => "hypot",
        BinOp::Mod => "mod",
        BinOp::Max => "fmax",
        BinOp::Min => "fmin",
    }
}

fn bin_from_token(t: &str) -> Option<BinOp> {
    Some(match t {
        "pow" => BinOp::Pow,
        "atan2" => BinOp::Atan2,
        "hypot" => BinOp::Hypot,
        "mod" => BinOp::Mod,
        "fmax" => BinOp::Max,
        "fmin" => BinOp::Min,
        _ => return None,
    })
}

fn grid_binary(b: Builtin) -> Option<BinOp> {
    use Builtin::*;
    Some(match b {
        GridPow => BinOp::Pow,
        GridAtan2 => BinOp::Atan2,
        GridHypot => BinOp::Hypot,
        GridMod => BinOp::Mod,
        GridMaximum => BinOp::Max,
        GridMinimum => BinOp::Min,
        _ => return None,
    })
}

pub(crate) fn grid_binary_token(b: Builtin) -> Option<&'static str> {
    grid_binary(b).map(bin_token)
}

/// Unary transcendental/rounding ops that fuse into the element-wise kernel, so
/// `tanh`, `exp`, `sigmoid` (= `1/(1+exp(-x))`), etc. compose into one pass and,
/// on the GPU backend, stay resident (one dispatch, no round-trip).
#[derive(Clone, Copy, PartialEq)]
enum UnOp {
    Exp, Exp2, Expm1, Ln, Log2, Log10, Log1p,
    Sin, Cos, Tan, Asin, Acos, Atan,
    Sinh, Cosh, Tanh, Asinh, Acosh, Atanh,
    Floor, Ceil, Round, Trunc,
    Sign, Cbrt, Recip, Neg, Degrees, Radians,
}

/// The CPU meaning of each unary op, matching the former `grid_map` closures
/// exactly (so fusing one changes no result).
#[inline]
fn apply_un(op: UnOp, x: f64) -> f64 {
    use UnOp::*;
    match op {
        Exp => x.exp(), Exp2 => x.exp2(), Expm1 => x.exp_m1(),
        Ln => x.ln(), Log2 => x.log2(), Log10 => x.log10(), Log1p => x.ln_1p(),
        Sin => x.sin(), Cos => x.cos(), Tan => x.tan(),
        Asin => x.asin(), Acos => x.acos(), Atan => x.atan(),
        Sinh => x.sinh(), Cosh => x.cosh(), Tanh => x.tanh(),
        Asinh => x.asinh(), Acosh => x.acosh(), Atanh => x.atanh(),
        Floor => x.floor(), Ceil => x.ceil(), Round => x.round_ties_even(), Trunc => x.trunc(),
        Sign => if x.is_nan() { x } else if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 },
        Cbrt => x.cbrt(), Recip => x.recip(), Neg => -x,
        Degrees => x.to_degrees(), Radians => x.to_radians(),
    }
}

/// SIMD form: `Neg` vectorizes; the transcendentals have no `f64x4` form, so they
/// go lane by lane (still one fused pass, just not vectorized for that op).
#[inline]
fn apply_un_simd(op: UnOp, v: f64x4) -> f64x4 {
    if op == UnOp::Neg {
        return -v;
    }
    let a = v.to_array();
    f64x4::from([apply_un(op, a[0]), apply_un(op, a[1]), apply_un(op, a[2]), apply_un(op, a[3])])
}

/// The `grid.eval` RPN token for a unary op (shared by the fuser and the parser).
fn un_token(op: UnOp) -> &'static str {
    use UnOp::*;
    match op {
        Exp => "exp", Exp2 => "exp2", Expm1 => "expm1", Ln => "ln", Log2 => "log2", Log10 => "log10", Log1p => "log1p",
        Sin => "sin", Cos => "cos", Tan => "tan", Asin => "asin", Acos => "acos", Atan => "atan",
        Sinh => "sinh", Cosh => "cosh", Tanh => "tanh", Asinh => "asinh", Acosh => "acosh", Atanh => "atanh",
        Floor => "floor", Ceil => "ceil", Round => "round", Trunc => "trunc",
        Sign => "sign", Cbrt => "cbrt", Recip => "recip", Neg => "neg", Degrees => "deg", Radians => "rad",
    }
}

fn un_from_token(t: &str) -> Option<UnOp> {
    use UnOp::*;
    Some(match t {
        "exp" => Exp, "exp2" => Exp2, "expm1" => Expm1, "ln" => Ln, "log2" => Log2, "log10" => Log10, "log1p" => Log1p,
        "sin" => Sin, "cos" => Cos, "tan" => Tan, "asin" => Asin, "acos" => Acos, "atan" => Atan,
        "sinh" => Sinh, "cosh" => Cosh, "tanh" => Tanh, "asinh" => Asinh, "acosh" => Acosh, "atanh" => Atanh,
        "floor" => Floor, "ceil" => Ceil, "round" => Round, "trunc" => Trunc,
        "sign" => Sign, "cbrt" => Cbrt, "recip" => Recip, "neg" => Neg, "deg" => Degrees, "rad" => Radians,
        _ => return None,
    })
}

/// The unary op a fuseable unary grid builtin maps to (None = not fuseable).
/// Single source of truth for both the fuser (which emits the token) and the
/// runtime (which executes it).
fn grid_unary(b: Builtin) -> Option<UnOp> {
    use Builtin::*;
    use UnOp as U;
    Some(match b {
        GridExp => U::Exp, GridExp2 => U::Exp2, GridExpm1 => U::Expm1,
        GridLog => U::Ln, GridLog2 => U::Log2, GridLog10 => U::Log10, GridLog1p => U::Log1p,
        GridSin => U::Sin, GridCos => U::Cos, GridTan => U::Tan,
        GridAsin => U::Asin, GridAcos => U::Acos, GridAtan => U::Atan,
        GridSinh => U::Sinh, GridCosh => U::Cosh, GridTanh => U::Tanh,
        GridAsinh => U::Asinh, GridAcosh => U::Acosh, GridAtanh => U::Atanh,
        GridFloor => U::Floor, GridCeil => U::Ceil, GridRound => U::Round, GridTrunc => U::Trunc,
        GridSign => U::Sign, GridCbrt => U::Cbrt, GridRecip => U::Recip, GridNeg => U::Neg,
        GridDegrees => U::Degrees, GridRadians => U::Radians,
        _ => return None,
    })
}

/// The fuser reaches `un_token` through this (it has a `Builtin`, not a `UnOp`).
pub(crate) fn grid_unary_token(b: Builtin) -> Option<&'static str> {
    grid_unary(b).map(un_token)
}

fn parse_program(src: &str) -> Result<Vec<Micro>, String> {
    src.split_whitespace()
        .map(|t| match t {
            "+" => Ok(Micro::Add),
            "-" => Ok(Micro::Sub),
            "*" => Ok(Micro::Mul),
            "/" => Ok(Micro::Div),
            "q" => Ok(Micro::Sqrt),
            "sq" => Ok(Micro::Square),
            "ab" => Ok(Micro::Abs),
            _ => {
                if let Some(op) = un_from_token(t) {
                    Ok(Micro::Un(op))
                } else if let Some(op) = bin_from_token(t) {
                    Ok(Micro::Bin(op))
                } else if let Some(k) = t.strip_prefix('l') {
                    k.parse().map(Micro::Leaf).map_err(|_| "nungw: bad kernel".to_string())
                } else if let Some(k) = t.strip_prefix('s') {
                    k.parse().map(Micro::Scalar).map_err(|_| "nungw: bad kernel".to_string())
                } else {
                    Err(format!("nungw: bad kernel token \"{}\"", t))
                }
            }
        })
        .collect()
}

#[inline]
fn run_program(program: &[Micro], leaves: &[&[f64]], scalars: &[f64], i: usize) -> f64 {
    let mut st = [0f64; 32];
    let mut sp = 0usize;
    for m in program {
        match m {
            Micro::Leaf(k) => {
                st[sp] = leaves[*k][i];
                sp += 1;
            }
            Micro::Scalar(k) => {
                st[sp] = scalars[*k];
                sp += 1;
            }
            Micro::Add => {
                sp -= 1;
                st[sp - 1] += st[sp];
            }
            Micro::Sub => {
                sp -= 1;
                st[sp - 1] -= st[sp];
            }
            Micro::Mul => {
                sp -= 1;
                st[sp - 1] *= st[sp];
            }
            Micro::Div => {
                sp -= 1;
                st[sp - 1] /= st[sp];
            }
            Micro::Sqrt => st[sp - 1] = st[sp - 1].sqrt(),
            Micro::Square => st[sp - 1] = st[sp - 1] * st[sp - 1],
            Micro::Abs => st[sp - 1] = st[sp - 1].abs(),
            Micro::Un(op) => st[sp - 1] = apply_un(*op, st[sp - 1]),
            Micro::Bin(op) => {
                sp -= 1;
                st[sp - 1] = apply_bin(*op, st[sp - 1], st[sp]);
            }
        }
    }
    st[0]
}

/// The same fused kernel, four lanes at a time with SIMD. `i` is the start of a
/// block of four contiguous elements (so `i + 3` must be in range). Every opcode
/// has a direct `f64x4` form, so the whole program vectorizes with no fallback.
fn run_program_simd(program: &[Micro], leaves: &[&[f64]], scalars: &[f64], i: usize) -> f64x4 {
    let mut st = [f64x4::ZERO; 32];
    let mut sp = 0usize;
    for m in program {
        match m {
            Micro::Leaf(k) => {
                let s = leaves[*k];
                st[sp] = f64x4::from([s[i], s[i + 1], s[i + 2], s[i + 3]]);
                sp += 1;
            }
            Micro::Scalar(k) => {
                st[sp] = f64x4::splat(scalars[*k]);
                sp += 1;
            }
            Micro::Add => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] + st[sp];
            }
            Micro::Sub => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] - st[sp];
            }
            Micro::Mul => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] * st[sp];
            }
            Micro::Div => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] / st[sp];
            }
            Micro::Sqrt => st[sp - 1] = st[sp - 1].sqrt(),
            Micro::Square => st[sp - 1] = st[sp - 1] * st[sp - 1],
            Micro::Abs => st[sp - 1] = st[sp - 1].abs(),
            Micro::Un(op) => st[sp - 1] = apply_un_simd(*op, st[sp - 1]),
            Micro::Bin(op) => {
                sp -= 1;
                st[sp - 1] = apply_bin_simd(*op, st[sp - 1], st[sp]);
            }
        }
    }
    st[0]
}

/// Transpiles a fused element-wise kernel (the RPN `Micro` program) into a WGSL
/// compute shader for the GPU backend: each invocation computes one output
/// element in f32. Scalars are baked in as literals, one read-only storage
/// buffer per leaf, one read_write output buffer. Returns None if a scalar is
/// non-finite (no WGSL literal) or the program is malformed -- the caller then
/// stays on the CPU. Compiled under the `gpu` feature (and under `test`, so the
/// transpiler can be unit-tested without pulling in wgpu).
/// The WGSL expression for a unary op applied to `a`. WGSL has most of these as
/// built-ins; the few it lacks (expm1, log10, log1p, cbrt, recip, deg/rad) are
/// composed. Matches `apply_un` up to f32 precision.
#[cfg(any(feature = "gpu", test))]
fn un_wgsl(op: UnOp, a: &str) -> String {
    use UnOp::*;
    match op {
        Exp => format!("exp({a})"),
        Exp2 => format!("exp2({a})"),
        Expm1 => format!("(exp({a}) - 1.0)"),
        Ln => format!("log({a})"),
        Log2 => format!("log2({a})"),
        Log10 => format!("(log({a}) * 0.4342944819032518)"),
        Log1p => format!("log(1.0 + {a})"),
        Sin => format!("sin({a})"),
        Cos => format!("cos({a})"),
        Tan => format!("tan({a})"),
        Asin => format!("asin({a})"),
        Acos => format!("acos({a})"),
        Atan => format!("atan({a})"),
        Sinh => format!("sinh({a})"),
        Cosh => format!("cosh({a})"),
        Tanh => format!("tanh({a})"),
        Asinh => format!("asinh({a})"),
        Acosh => format!("acosh({a})"),
        Atanh => format!("atanh({a})"),
        Floor => format!("floor({a})"),
        Ceil => format!("ceil({a})"),
        Round => format!("round({a})"),
        Trunc => format!("trunc({a})"),
        Sign => format!("sign({a})"),
        Cbrt => format!("(sign({a}) * pow(abs({a}), 0.3333333333333333))"),
        Recip => format!("(1.0 / {a})"),
        Neg => format!("(-{a})"),
        Degrees => format!("({a} * 57.29577951308232)"),
        Radians => format!("({a} * 0.017453292519943295)"),
    }
}

/// The WGSL expression for a binary op of `a` and `b`. Matches `apply_bin` up to
/// f32 precision (pow of a negative base is NaN on the GPU, as in WGSL).
#[cfg(any(feature = "gpu", test))]
fn bin_wgsl(op: BinOp, a: &str, b: &str) -> String {
    match op {
        BinOp::Pow => format!("pow({a}, {b})"),
        BinOp::Atan2 => format!("atan2({a}, {b})"),
        BinOp::Hypot => format!("sqrt({a} * {a} + {b} * {b})"),
        BinOp::Mod => format!("({a} - floor({a} / {b}) * {b})"),
        BinOp::Max => format!("max({a}, {b})"),
        BinOp::Min => format!("min({a}, {b})"),
    }
}

#[cfg(any(feature = "gpu", test))]
fn program_to_wgsl(program: &[Micro], scalars: &[f64], n: usize) -> Option<String> {
    let n_leaves = program
        .iter()
        .filter_map(|m| if let Micro::Leaf(k) = m { Some(*k + 1) } else { None })
        .max()
        .unwrap_or(0);
    let mut stack: Vec<String> = Vec::new();
    for m in program {
        match m {
            Micro::Leaf(k) => stack.push(format!("in{}[i]", k)),
            Micro::Scalar(k) => {
                let v = *scalars.get(*k)?;
                if !v.is_finite() {
                    return None;
                }
                stack.push(format!("f32({:?})", v as f32));
            }
            Micro::Add => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(format!("({} + {})", a, b));
            }
            Micro::Sub => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(format!("({} - {})", a, b));
            }
            Micro::Mul => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(format!("({} * {})", a, b));
            }
            Micro::Div => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(format!("({} / {})", a, b));
            }
            Micro::Sqrt => {
                let a = stack.pop()?;
                stack.push(format!("sqrt({})", a));
            }
            Micro::Square => {
                let a = stack.pop()?;
                stack.push(format!("({0} * {0})", a));
            }
            Micro::Abs => {
                let a = stack.pop()?;
                stack.push(format!("abs({})", a));
            }
            Micro::Un(op) => {
                let a = stack.pop()?;
                stack.push(un_wgsl(*op, &a));
            }
            Micro::Bin(op) => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(bin_wgsl(*op, &a, &b));
            }
        }
    }
    let expr = stack.pop()?;
    if !stack.is_empty() {
        return None; // the program did not reduce to a single value
    }
    let mut src = String::new();
    for k in 0..n_leaves {
        src.push_str(&format!(
            "@group(0) @binding({k}) var<storage, read> in{k}: array<f32>;\n"
        ));
    }
    src.push_str(&format!(
        "@group(0) @binding({n_leaves}) var<storage, read_write> outp: array<f32>;\n"
    ));
    src.push_str("@compute @workgroup_size(256)\n");
    src.push_str("fn main(@builtin(global_invocation_id) gid: vec3<u32>) {\n");
    src.push_str("  let i = gid.x;\n");
    src.push_str(&format!("  if (i >= {n}u) {{ return; }}\n"));
    src.push_str(&format!("  outp[i] = {expr};\n"));
    src.push_str("}\n");
    Some(src)
}

/// Like `program_to_wgsl`, but with NumPy-style broadcasting: each leaf is read
/// through its broadcast strides against `out_shape`. The output shape and each
/// leaf's strides (a size-1 dimension has stride 0, so it repeats) are baked into
/// the shader as literals, so the kernel decodes the multi-index and reads each
/// leaf at its offset. Handles a bias add `z + b` where `b` repeats over rows.
#[cfg(any(feature = "gpu", test))]
fn program_to_wgsl_bcast(program: &[Micro], scalars: &[f64], out_shape: &[usize], strides: &[Vec<usize>]) -> Option<String> {
    let n_leaves = program
        .iter()
        .filter_map(|m| if let Micro::Leaf(k) = m { Some(*k + 1) } else { None })
        .max()
        .unwrap_or(0);
    if strides.len() < n_leaves {
        return None;
    }
    let ndim = out_shape.len();
    let total: usize = out_shape.iter().product();
    let mut stack: Vec<String> = Vec::new();
    for m in program {
        match m {
            Micro::Leaf(k) => stack.push(format!("in{}[off{}]", k, k)),
            Micro::Scalar(k) => {
                let v = *scalars.get(*k)?;
                if !v.is_finite() {
                    return None;
                }
                stack.push(format!("f32({:?})", v as f32));
            }
            Micro::Add => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(format!("({} + {})", a, b));
            }
            Micro::Sub => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(format!("({} - {})", a, b));
            }
            Micro::Mul => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(format!("({} * {})", a, b));
            }
            Micro::Div => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(format!("({} / {})", a, b));
            }
            Micro::Sqrt => {
                let a = stack.pop()?;
                stack.push(format!("sqrt({})", a));
            }
            Micro::Square => {
                let a = stack.pop()?;
                stack.push(format!("({0} * {0})", a));
            }
            Micro::Abs => {
                let a = stack.pop()?;
                stack.push(format!("abs({})", a));
            }
            Micro::Un(op) => {
                let a = stack.pop()?;
                stack.push(un_wgsl(*op, &a));
            }
            Micro::Bin(op) => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(bin_wgsl(*op, &a, &b));
            }
        }
    }
    let expr = stack.pop()?;
    if !stack.is_empty() {
        return None;
    }
    let mut src = String::new();
    for k in 0..n_leaves {
        src.push_str(&format!("@group(0) @binding({k}) var<storage, read> in{k}: array<f32>;\n"));
    }
    src.push_str(&format!("@group(0) @binding({n_leaves}) var<storage, read_write> outp: array<f32>;\n"));
    src.push_str("@compute @workgroup_size(256)\n");
    src.push_str("fn main(@builtin(global_invocation_id) gid: vec3<u32>) {\n");
    src.push_str("  let i = gid.x;\n");
    src.push_str(&format!("  if (i >= {total}u) {{ return; }}\n"));
    src.push_str("  var rem = i;\n");
    for d in (1..ndim).rev() {
        src.push_str(&format!("  let idx{d} = rem % {0}u; rem = rem / {0}u;\n", out_shape[d]));
    }
    src.push_str("  let idx0 = rem;\n");
    for k in 0..n_leaves {
        let terms: Vec<String> = (0..ndim)
            .filter(|&d| strides[k][d] != 0)
            .map(|d| format!("idx{} * {}u", d, strides[k][d]))
            .collect();
        let off = if terms.is_empty() { "0u".to_string() } else { terms.join(" + ") };
        src.push_str(&format!("  let off{k} = {off};\n"));
    }
    src.push_str(&format!("  outp[i] = {expr};\n"));
    src.push_str("}\n");
    Some(src)
}

// Set by `nungw.gpu(on)`; read by the GPU routing. Always compiled (the switch
// exists whether or not the `gpu` feature is on; without it, it just reports the
// GPU as unavailable).
static GPU_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "gpu")]
fn gpu_switched_on() -> bool {
    GPU_REQUESTED.load(std::sync::atomic::Ordering::Relaxed)
        || std::env::var("GLOBW_GPU").ok().as_deref() == Some("1")
}

// Tries a matrix multiply on the GPU (f32). The user-facing `nungw.gpu(true)`
// switch turns this on; matmul is O(n^3) compute vs O(n^2) transfer, so it wins
// at large sizes even re-uploading each call. Small products stay on the CPU
// (BLAS/kernel). None -> CPU fallback.
#[cfg(feature = "gpu")]
fn try_gpu_matmul(va: &[f64], vb: &[f64], rows: usize, inner: usize, cols: usize) -> Option<Vec<f64>> {
    if !gpu_switched_on() {
        return None;
    }
    if rows.saturating_mul(inner).saturating_mul(cols) < (1 << 20) {
        return None;
    }
    let a32: Vec<f32> = va.iter().map(|&x| x as f32).collect();
    let b32: Vec<f32> = vb.iter().map(|&x| x as f32).collect();
    let out = crate::gpu::run_matmul(&a32, &b32, rows, inner, cols)?;
    Some(out.into_iter().map(|x| x as f64).collect())
}

// Resident matmul: if the operands are GPU-resident, multiply them on-device and
// keep the result resident. `nungw.matmul(a, b)` (dims from shapes) or the
// explicit `matmul(a, rows, inner, b, cols)`. Returns:
//   Ok(None)        -> both operands are on the CPU (use the normal path)
//   Ok(Some(gpu))   -> both resident, computed on the GPU
//   Err(..)         -> one on the GPU and one on the CPU (ask to move them)
#[cfg(feature = "gpu")]
fn try_matmul_resident(args: &[Value]) -> Result<Option<Value>, String> {
    let b_idx = if args.len() == 2 { 1 } else { 3 };
    let a_gpu = matches!(&args[0], Value::GpuFloats(..));
    let b_gpu = matches!(&args[b_idx], Value::GpuFloats(..));
    if !a_gpu && !b_gpu {
        return Ok(None);
    }
    let (Value::GpuFloats(ha, sa), Value::GpuFloats(hb, sb)) = (&args[0], &args[b_idx]) else {
        return Err("nungw.matmul: one operand is on the GPU and the other on the CPU; move both with nungw.to_gpu / nungw.to_cpu".to_string());
    };
    let (rows, inner, cols) = if args.len() == 2 {
        if sa.len() != 2 || sb.len() != 2 {
            return Err("nungw.matmul: resident operands must be 2-D (or pass dimensions explicitly)".to_string());
        }
        if sb[0] != sa[1] {
            return Err(format!("nungw.matmul: inner dimensions do not match ({}x{} times {}x{})", sa[0], sa[1], sb[0], sb[1]));
        }
        (sa[0], sa[1], sb[1])
    } else {
        let (r, i, c) = (int(&args[1]), int(&args[2]), int(&args[4]));
        if r < 0 || i < 0 || c < 0 {
            return Err("nungw.matmul: the dimensions cannot be negative".to_string());
        }
        (r as usize, i as usize, c as usize)
    };
    let h = crate::gpu::matmul_resident(ha, hb, rows, inner, cols).ok_or("nungw.matmul: the GPU multiply failed")?;
    Ok(Some(Value::GpuFloats(h, Rc::new(vec![rows, cols]))))
}

// Resident matmul_ta (a^T·b) / matmul_tb (a·b^T). Same contract as
// try_matmul_resident. `ta`/`tb` pick the transpose.
#[cfg(feature = "gpu")]
fn try_matmul_ta_tb_resident(args: &[Value], ta: bool, tb: bool) -> Result<Option<Value>, String> {
    let b_idx = if args.len() == 2 { 1 } else { 3 };
    let a_gpu = matches!(&args[0], Value::GpuFloats(..));
    let b_gpu = matches!(&args[b_idx], Value::GpuFloats(..));
    if !a_gpu && !b_gpu {
        return Ok(None);
    }
    let (Value::GpuFloats(ha, sa), Value::GpuFloats(hb, sb)) = (&args[0], &args[b_idx]) else {
        return Err("nungw: one operand is on the GPU and the other on the CPU; move both with nungw.to_gpu / nungw.to_cpu".to_string());
    };
    // result is rows×cols, with inner the summed dimension (op(A) rows×inner, op(B) inner×cols)
    let (rows, inner, cols) = if ta {
        // a^T·b : a stored [a_rows × a_cols], b [a_rows × b_cols] -> [a_cols × b_cols]
        if args.len() == 2 {
            if sa.len() != 2 || sb.len() != 2 {
                return Err("nungw.matmul_ta: resident operands must be 2-D".to_string());
            }
            if sb[0] != sa[0] {
                return Err("nungw.matmul_ta: the second matrix must have as many rows as the first".to_string());
            }
            (sa[1], sa[0], sb[1])
        } else {
            (int(&args[2]) as usize, int(&args[1]) as usize, int(&args[4]) as usize)
        }
    } else {
        let _ = tb;
        // a·b^T : a [a_rows × inner], b stored [b_rows × inner] -> [a_rows × b_rows]
        if args.len() == 2 {
            if sa.len() != 2 || sb.len() != 2 {
                return Err("nungw.matmul_tb: resident operands must be 2-D".to_string());
            }
            if sb[1] != sa[1] {
                return Err("nungw.matmul_tb: both matrices must have the same number of columns".to_string());
            }
            (sa[0], sa[1], sb[0])
        } else {
            (int(&args[1]) as usize, int(&args[2]) as usize, int(&args[4]) as usize)
        }
    };
    let flags = (ta as u32) | ((tb as u32) << 1);
    let h = crate::gpu::matmul_resident_t(ha, hb, rows, inner, cols, flags).ok_or("nungw: the GPU multiply failed")?;
    Ok(Some(Value::GpuFloats(h, Rc::new(vec![rows, cols]))))
}

// Tries to run a fused equal-shape element-wise kernel on the GPU (f32). Opt-in
// via GLOBW_GPU=1 and bounded to a size window: below ~1M the CPU wins (upload +
// dispatch overhead), and above the one-dimensional dispatch limit we stay on
// the CPU. Returns None whenever the GPU is unavailable or anything fails, so
// the caller falls back to the SIMD CPU path.
#[cfg(feature = "gpu")]
fn try_gpu_fused(program: &[Micro], leaves: &[&[f64]], scalars: &[f64], n: usize) -> Option<Vec<f64>> {
    if std::env::var("GLOBW_GPU").ok().as_deref() != Some("1") {
        return None;
    }
    const MAX_N: usize = 65535 * 256; // one-D workgroup dispatch limit at size 256
    if n < (1 << 20) || n > MAX_N || leaves.is_empty() {
        return None;
    }
    let wgsl = program_to_wgsl(program, scalars, n)?;
    let inputs: Vec<Vec<f32>> = leaves.iter().map(|l| l.iter().map(|&x| x as f32).collect()).collect();
    let out = crate::gpu::run_elementwise(&wgsl, &inputs, n)?;
    Some(out.into_iter().map(|x| x as f64).collect())
}

// A fused kernel over one contiguous run along the last axis, for the broadcast
// path. Within such a run every leaf's last-axis stride is either 1 (it varies
// with the position -> read contiguously) or 0 (constant over the run -> splat);
// `bases[k]` is leaf k's flat offset at the start of the run, `contig[k]` which
// of the two it is. This lets the broadcast path vectorize too (e.g. a bias add
// `z[n,m] + b[m]`), not just the equal-shape path.
#[inline]
fn run_row(program: &[Micro], leaves: &[&[f64]], scalars: &[f64], bases: &[usize], contig: &[bool], j: usize) -> f64 {
    let mut st = [0f64; 32];
    let mut sp = 0usize;
    for m in program {
        match m {
            Micro::Leaf(k) => {
                let off = bases[*k] + if contig[*k] { j } else { 0 };
                st[sp] = leaves[*k][off];
                sp += 1;
            }
            Micro::Scalar(k) => {
                st[sp] = scalars[*k];
                sp += 1;
            }
            Micro::Add => {
                sp -= 1;
                st[sp - 1] += st[sp];
            }
            Micro::Sub => {
                sp -= 1;
                st[sp - 1] -= st[sp];
            }
            Micro::Mul => {
                sp -= 1;
                st[sp - 1] *= st[sp];
            }
            Micro::Div => {
                sp -= 1;
                st[sp - 1] /= st[sp];
            }
            Micro::Sqrt => st[sp - 1] = st[sp - 1].sqrt(),
            Micro::Square => st[sp - 1] = st[sp - 1] * st[sp - 1],
            Micro::Abs => st[sp - 1] = st[sp - 1].abs(),
            Micro::Un(op) => st[sp - 1] = apply_un(*op, st[sp - 1]),
            Micro::Bin(op) => {
                sp -= 1;
                st[sp - 1] = apply_bin(*op, st[sp - 1], st[sp]);
            }
        }
    }
    st[0]
}

#[inline]
fn run_row_simd(program: &[Micro], leaves: &[&[f64]], scalars: &[f64], bases: &[usize], contig: &[bool], j: usize) -> f64x4 {
    let mut st = [f64x4::ZERO; 32];
    let mut sp = 0usize;
    for m in program {
        match m {
            Micro::Leaf(k) => {
                let b = bases[*k];
                st[sp] = if contig[*k] {
                    let s = leaves[*k];
                    f64x4::from([s[b + j], s[b + j + 1], s[b + j + 2], s[b + j + 3]])
                } else {
                    f64x4::splat(leaves[*k][b])
                };
                sp += 1;
            }
            Micro::Scalar(k) => {
                st[sp] = f64x4::splat(scalars[*k]);
                sp += 1;
            }
            Micro::Add => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] + st[sp];
            }
            Micro::Sub => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] - st[sp];
            }
            Micro::Mul => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] * st[sp];
            }
            Micro::Div => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] / st[sp];
            }
            Micro::Sqrt => st[sp - 1] = st[sp - 1].sqrt(),
            Micro::Square => st[sp - 1] = st[sp - 1] * st[sp - 1],
            Micro::Abs => st[sp - 1] = st[sp - 1].abs(),
            Micro::Un(op) => st[sp - 1] = apply_un_simd(*op, st[sp - 1]),
            Micro::Bin(op) => {
                sp -= 1;
                st[sp - 1] = apply_bin_simd(*op, st[sp - 1], st[sp]);
            }
        }
    }
    st[0]
}

// ---- SIMD reductions -------------------------------------------------------
// These reductions used to be scalar `par_iter().sum()` etc. They now fold four
// lanes at a time (f64x4) with a short scalar tail; the `reduce_*` wrappers keep
// the rayon parallelism by vectorizing each big chunk in parallel. The lane-wise
// order can differ from a strict left-to-right scalar sum in the last ULP -- but
// the previous reduction was already parallel (unordered), so nothing that was
// exact becomes inexact.

#[inline]
fn simd_sum_seq(c: &[f64]) -> f64 {
    let mut acc = f64x4::ZERO;
    let mut it = c.chunks_exact(4);
    for b in it.by_ref() {
        acc = acc + f64x4::from([b[0], b[1], b[2], b[3]]);
    }
    let a = acc.to_array();
    let mut s = a[0] + a[1] + a[2] + a[3];
    for &x in it.remainder() {
        s += x;
    }
    s
}

#[inline]
fn simd_prod_seq(c: &[f64]) -> f64 {
    let mut acc = f64x4::splat(1.0);
    let mut it = c.chunks_exact(4);
    for b in it.by_ref() {
        acc = acc * f64x4::from([b[0], b[1], b[2], b[3]]);
    }
    let a = acc.to_array();
    let mut p = a[0] * a[1] * a[2] * a[3];
    for &x in it.remainder() {
        p *= x;
    }
    p
}

#[inline]
fn simd_sumsq_seq(c: &[f64]) -> f64 {
    let mut acc = f64x4::ZERO;
    let mut it = c.chunks_exact(4);
    for b in it.by_ref() {
        let v = f64x4::from([b[0], b[1], b[2], b[3]]);
        acc = acc + v * v;
    }
    let a = acc.to_array();
    let mut s = a[0] + a[1] + a[2] + a[3];
    for &x in it.remainder() {
        s += x * x;
    }
    s
}

#[inline]
fn simd_sumsqdev_seq(c: &[f64], mean: f64) -> f64 {
    let m = f64x4::splat(mean);
    let mut acc = f64x4::ZERO;
    let mut it = c.chunks_exact(4);
    for b in it.by_ref() {
        let d = f64x4::from([b[0], b[1], b[2], b[3]]) - m;
        acc = acc + d * d;
    }
    let a = acc.to_array();
    let mut s = a[0] + a[1] + a[2] + a[3];
    for &x in it.remainder() {
        let d = x - mean;
        s += d * d;
    }
    s
}

#[inline]
fn simd_dot_seq(a: &[f64], b: &[f64]) -> f64 {
    let mut acc = f64x4::ZERO;
    let mut ia = a.chunks_exact(4);
    let mut ib = b.chunks_exact(4);
    for (x, y) in ia.by_ref().zip(ib.by_ref()) {
        acc = acc + f64x4::from([x[0], x[1], x[2], x[3]]) * f64x4::from([y[0], y[1], y[2], y[3]]);
    }
    let arr = acc.to_array();
    let mut s = arr[0] + arr[1] + arr[2] + arr[3];
    for (x, y) in ia.remainder().iter().zip(ib.remainder()) {
        s += x * y;
    }
    s
}

// Chunk size for the parallel reductions: a multiple of 4 (so each chunk's SIMD
// body leaves only a short scalar tail), large enough to amortize rayon's
// per-task overhead.
#[cfg(feature = "native")]
const REDUCE_CHUNK: usize = 1 << 14;

#[cfg(feature = "native")]
fn reduce_sum(v: &[f64]) -> f64 {
    v.par_chunks(REDUCE_CHUNK).map(simd_sum_seq).sum()
}
#[cfg(not(feature = "native"))]
fn reduce_sum(v: &[f64]) -> f64 {
    simd_sum_seq(v)
}

#[cfg(feature = "native")]
fn reduce_prod(v: &[f64]) -> f64 {
    v.par_chunks(REDUCE_CHUNK).map(simd_prod_seq).product()
}
#[cfg(not(feature = "native"))]
fn reduce_prod(v: &[f64]) -> f64 {
    simd_prod_seq(v)
}

#[cfg(feature = "native")]
fn reduce_sumsq(v: &[f64]) -> f64 {
    v.par_chunks(REDUCE_CHUNK).map(simd_sumsq_seq).sum()
}
#[cfg(not(feature = "native"))]
fn reduce_sumsq(v: &[f64]) -> f64 {
    simd_sumsq_seq(v)
}

#[cfg(feature = "native")]
fn reduce_sumsqdev(v: &[f64], mean: f64) -> f64 {
    v.par_chunks(REDUCE_CHUNK).map(|c| simd_sumsqdev_seq(c, mean)).sum()
}
#[cfg(not(feature = "native"))]
fn reduce_sumsqdev(v: &[f64], mean: f64) -> f64 {
    simd_sumsqdev_seq(v, mean)
}

#[cfg(feature = "native")]
fn reduce_dot(a: &[f64], b: &[f64]) -> f64 {
    a.par_chunks(REDUCE_CHUNK)
        .zip(b.par_chunks(REDUCE_CHUNK))
        .map(|(x, y)| simd_dot_seq(x, y))
        .sum()
}
#[cfg(not(feature = "native"))]
fn reduce_dot(a: &[f64], b: &[f64]) -> f64 {
    simd_dot_seq(a, b)
}

// In-place variants of the fused kernel: leaf `self_leaf` is read from the output
// buffer itself (its current value, passed in), the rest from `leaves`. This lets
// the output reuse a dead input buffer (element-wise: out[i] = f(.., out[i], ..)
// is safe because each element only reads its own position).
#[inline]
fn run_program_inplace(program: &[Micro], leaves: &[&[f64]], scalars: &[f64], i: usize, self_leaf: usize, self_val: f64) -> f64 {
    let mut st = [0f64; 32];
    let mut sp = 0usize;
    for m in program {
        match m {
            Micro::Leaf(k) => {
                st[sp] = if *k == self_leaf { self_val } else { leaves[*k][i] };
                sp += 1;
            }
            Micro::Scalar(k) => {
                st[sp] = scalars[*k];
                sp += 1;
            }
            Micro::Add => {
                sp -= 1;
                st[sp - 1] += st[sp];
            }
            Micro::Sub => {
                sp -= 1;
                st[sp - 1] -= st[sp];
            }
            Micro::Mul => {
                sp -= 1;
                st[sp - 1] *= st[sp];
            }
            Micro::Div => {
                sp -= 1;
                st[sp - 1] /= st[sp];
            }
            Micro::Sqrt => st[sp - 1] = st[sp - 1].sqrt(),
            Micro::Square => st[sp - 1] = st[sp - 1] * st[sp - 1],
            Micro::Abs => st[sp - 1] = st[sp - 1].abs(),
            Micro::Un(op) => st[sp - 1] = apply_un(*op, st[sp - 1]),
            Micro::Bin(op) => {
                sp -= 1;
                st[sp - 1] = apply_bin(*op, st[sp - 1], st[sp]);
            }
        }
    }
    st[0]
}

#[inline]
fn run_program_simd_inplace(program: &[Micro], leaves: &[&[f64]], scalars: &[f64], i: usize, self_leaf: usize, self_blk: f64x4) -> f64x4 {
    let mut st = [f64x4::ZERO; 32];
    let mut sp = 0usize;
    for m in program {
        match m {
            Micro::Leaf(k) => {
                st[sp] = if *k == self_leaf {
                    self_blk
                } else {
                    let s = leaves[*k];
                    f64x4::from([s[i], s[i + 1], s[i + 2], s[i + 3]])
                };
                sp += 1;
            }
            Micro::Scalar(k) => {
                st[sp] = f64x4::splat(scalars[*k]);
                sp += 1;
            }
            Micro::Add => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] + st[sp];
            }
            Micro::Sub => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] - st[sp];
            }
            Micro::Mul => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] * st[sp];
            }
            Micro::Div => {
                sp -= 1;
                st[sp - 1] = st[sp - 1] / st[sp];
            }
            Micro::Sqrt => st[sp - 1] = st[sp - 1].sqrt(),
            Micro::Square => st[sp - 1] = st[sp - 1] * st[sp - 1],
            Micro::Abs => st[sp - 1] = st[sp - 1].abs(),
            Micro::Un(op) => st[sp - 1] = apply_un_simd(*op, st[sp - 1]),
            Micro::Bin(op) => {
                sp -= 1;
                st[sp - 1] = apply_bin_simd(*op, st[sp - 1], st[sp]);
            }
        }
    }
    st[0]
}

// Fills `dst` in place with the fused kernel, where `dst` IS leaf `self_leaf`
// (read from dst itself, before overwriting). Other leaves read at `base + local`.
fn fill_fused_inplace(dst: &mut [f64], base: usize, program: &[Micro], leaves: &[&[f64]], scalars: &[f64], self_leaf: usize) {
    let len = dst.len();
    let full = len / 4;
    for ci in 0..full {
        let j = ci * 4;
        let blk = f64x4::from([dst[j], dst[j + 1], dst[j + 2], dst[j + 3]]);
        let r = run_program_simd_inplace(program, leaves, scalars, base + j, self_leaf, blk).to_array();
        dst[j..j + 4].copy_from_slice(&r);
    }
    for j in (full * 4)..len {
        let sv = dst[j];
        dst[j] = run_program_inplace(program, leaves, scalars, base + j, self_leaf, sv);
    }
}

// Fills `dst` with the fused element-wise kernel, where dst[j] is output element
// `base + j`. SIMD four at a time plus a scalar tail.
#[inline]
fn fill_fused_range(dst: &mut [f64], base: usize, program: &[Micro], leaves: &[&[f64]], scalars: &[f64]) {
    let len = dst.len();
    let full = len / 4;
    for ci in 0..full {
        let r = run_program_simd(program, leaves, scalars, base + ci * 4).to_array();
        dst[ci * 4..ci * 4 + 4].copy_from_slice(&r);
    }
    for j in (full * 4)..len {
        dst[j] = run_program(program, leaves, scalars, base + j);
    }
}

// Builtins that handle a GPU-resident array themselves (keep it on the GPU).
// Every other builtin gets its GpuFloats arguments downloaded to the CPU first.
#[cfg(feature = "gpu")]
fn gpu_resident_aware(b: Builtin) -> bool {
    use Builtin::*;
    matches!(
        b,
        GridEval | GridMatmul | GridMatmulTA | GridMatmulTB | GridToGpu | GridToCpu | GridShape | GridNdim | GridReshape
            | GridSumAxis | GridMeanAxis | GridMaxAxis | GridMinAxis | GridProdAxis
            | GridSum | GridMean | GridMax | GridMin | GridProd
    )
}

// A per-axis reduction of a 2-D resident array, staying resident.
#[cfg(feature = "gpu")]
fn gpu_axis_reduce(h: &crate::gpu::GpuHandle, shape: &[usize], axis: i64, op: crate::gpu::ReduceOp, name: &str) -> Result<Value, String> {
    if shape.len() != 2 {
        return Err(format!("nungw.{name}: a GPU-resident array must be 2-D here (use nungw.to_cpu)"));
    }
    if axis != 0 && axis != 1 {
        return Err(format!("nungw.{name}: the axis must be 0 or 1 for a 2-D array"));
    }
    let (rows, cols) = (shape[0], shape[1]);
    let out_h = crate::gpu::axis_reduce_resident(h, rows, cols, axis as usize, op)
        .ok_or_else(|| format!("nungw.{name}: the GPU reduction failed"))?;
    let out_shape = if axis == 0 { vec![cols] } else { vec![rows] };
    Ok(Value::GpuFloats(out_h, Rc::new(out_shape)))
}

// Downloads every GpuFloats argument to a CPU `Floats`, leaving the rest as-is,
// so a builtin that cannot run on the GPU still works (it just comes back to the
// CPU first). Used for ops that are not resident-aware.
#[cfg(feature = "gpu")]
fn gpu_download_args(args: &[Value]) -> Result<Vec<Value>, String> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        match a {
            Value::GpuFloats(h, shape) => {
                let f32v = crate::gpu::download(h).ok_or("nungw: cannot bring a GPU array back to the CPU")?;
                let f64v: Vec<f64> = f32v.iter().map(|&x| x as f64).collect();
                out.push(grid_val_shaped(f64v, (**shape).clone()));
            }
            other => out.push(other.clone()),
        }
    }
    Ok(out)
}

// Resident fused element-wise: all grid operands are GPU-resident, so transpile
// the fused program to WGSL and run it on their buffers, keeping the result
// resident. Equal shapes use the fast contiguous kernel; broadcastable shapes
// bake per-leaf strides into the kernel.
#[cfg(feature = "gpu")]
fn grid_eval_resident(program: &[Micro], rest: &[Value]) -> Result<Value, String> {
    let is_grid = |v: &&Value| matches!(v, Value::Floats(..) | Value::GpuFloats(..));
    let grid_vals: Vec<&Value> = rest.iter().filter(is_grid).collect();
    if grid_vals.iter().any(|v| matches!(v, Value::Floats(..))) {
        return Err("this nungw expression mixes CPU and GPU arrays; move them to the same device with nungw.to_gpu / nungw.to_cpu".to_string());
    }
    let scalars: Vec<f64> = rest.iter().filter(|v| !matches!(v, Value::Floats(..) | Value::GpuFloats(..))).map(real).collect();
    let shapes: Vec<Rc<Vec<usize>>> = grid_vals.iter().map(|v| shape_of(v)).collect();
    // the output shape is the broadcast of every operand's shape (NumPy rules)
    let mut out_shape = (*shapes[0]).clone();
    for s in &shapes[1..] {
        out_shape = broadcast_shape(&out_shape, s)?;
    }
    let total: usize = out_shape.iter().product();
    let handles: Vec<&crate::gpu::GpuHandle> = grid_vals
        .iter()
        .map(|v| match v {
            Value::GpuFloats(h, _) => &**h,
            _ => unreachable!(),
        })
        .collect();
    // equal shapes: the fast contiguous kernel; otherwise bake per-leaf broadcast
    // strides into the kernel so each leaf is read through them.
    let equal = shapes.iter().all(|s| s.iter().product::<usize>() == total && **s == out_shape);
    let wgsl = if equal {
        program_to_wgsl(program, &scalars, total)
    } else {
        let strides: Vec<Vec<usize>> = shapes.iter().map(|s| broadcast_strides(s, &out_shape)).collect();
        program_to_wgsl_bcast(program, &scalars, &out_shape, &strides)
    }
    .ok_or("nungw: cannot build the GPU kernel for this expression")?;
    let out = crate::gpu::run_elementwise_resident(&wgsl, &handles, total).ok_or("nungw: the GPU element-wise kernel failed")?;
    Ok(Value::GpuFloats(out, Rc::new(out_shape)))
}

fn grid_eval(program: &Value, rest: &[Value]) -> Result<Value, String> {
    let program = parse_program(text(program))?;
    #[cfg(feature = "gpu")]
    if rest.iter().any(|v| matches!(v, Value::GpuFloats(..))) {
        return grid_eval_resident(&program, rest);
    }
    let grid_vals: Vec<&Value> = rest.iter().filter(|v| matches!(v, Value::Floats(..))).collect();
    let borrows: Vec<std::cell::Ref<Vec<f64>>> = grid_vals.iter().map(|v| floats(v).borrow()).collect();
    let leaves: Vec<&[f64]> = borrows.iter().map(|r| r.as_slice()).collect();
    let scalars: Vec<f64> = rest.iter().filter(|v| !matches!(v, Value::Floats(..))).map(real).collect();
    let n = leaves.first().map_or(0, |l| l.len());

    if leaves.iter().all(|l| l.len() == n) {
        // Fast path: every array has the same size, so it is a plain element-wise
        // pass. SIMD: four contiguous elements per step across the cores, then a
        // short scalar tail for the last n % 4. The result keeps the first shape.
        let shape = grid_vals.first().map_or_else(|| vec![n], |v| (*shape_of(v)).clone());
        #[cfg(feature = "gpu")]
        if let Some(out) = try_gpu_fused(&program, &leaves, &scalars, n) {
            return Ok(grid_val_shaped(out, shape));
        }
        // In-place reuse: if a leaf is uniquely owned, it is a dead temporary (call
        // arguments live in fresh registers, so strong_count == 1 means only this
        // argument holds it -- never a live variable). Write the result into its
        // buffer instead of allocating. Element-wise, so aliasing the output with
        // that leaf at each index is safe.
        #[cfg(feature = "native")]
        if n > 0 {
            if let Some(ri) = grid_vals.iter().position(|v| Rc::strong_count(floats(v)) == 1) {
                let reuse_rc = floats(grid_vals[ri]).clone();
                drop(leaves);
                drop(borrows);
                let others: Vec<std::cell::Ref<Vec<f64>>> = grid_vals
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != ri)
                    .map(|(_, v)| floats(v).borrow())
                    .collect();
                let mut lv: Vec<&[f64]> = Vec::with_capacity(grid_vals.len());
                let mut oi = 0;
                for i in 0..grid_vals.len() {
                    if i == ri {
                        lv.push(&[]); // leaf ri is read from the output buffer, never here
                    } else {
                        lv.push(others[oi].as_slice());
                        oi += 1;
                    }
                }
                {
                    let mut buf = reuse_rc.borrow_mut();
                    const FUSE_CHUNK: usize = 1 << 14;
                    const FUSE_PAR_MIN: usize = 1 << 16;
                    if n >= FUSE_PAR_MIN {
                        buf.par_chunks_mut(FUSE_CHUNK).enumerate().for_each(|(ci, chunk)| {
                            fill_fused_inplace(chunk, ci * FUSE_CHUNK, &program, &lv, &scalars, ri);
                        });
                    } else {
                        fill_fused_inplace(&mut buf, 0, &program, &lv, &scalars, ri);
                    }
                }
                return Ok(Value::Floats(reuse_rc, Rc::new(shape)));
            }
        }
        // No zero-fill: every element is written below. Rayon only pays off on big
        // arrays; below the threshold a sequential SIMD pass avoids its overhead
        // (this is where small per-op work, e.g. a neural net's layers, lives).
        const FUSE_CHUNK: usize = 1 << 14; // multiple of 4
        const FUSE_PAR_MIN: usize = 1 << 16;
        let mut data: Vec<f64> = Vec::with_capacity(n);
        // SAFETY: fill_fused_range writes every element of each chunk, covering 0..n.
        unsafe { data.set_len(n) };
        #[cfg(feature = "native")]
        if n >= FUSE_PAR_MIN {
            data.par_chunks_mut(FUSE_CHUNK).enumerate().for_each(|(ci, dst)| {
                fill_fused_range(dst, ci * FUSE_CHUNK, &program, &leaves, &scalars);
            });
        } else {
            fill_fused_range(&mut data, 0, &program, &leaves, &scalars);
        }
        #[cfg(not(feature = "native"))]
        fill_fused_range(&mut data, 0, &program, &leaves, &scalars);
        return Ok(grid_val_shaped(data, shape));
    }

    // Broadcast path: the arrays have different shapes. Align them from the right
    // (NumPy rules); a dimension of size 1 repeats. Each output element reads each
    // leaf through its broadcast strides. Parallel, one pass, no temporaries.
    let shapes: Vec<Rc<Vec<usize>>> = grid_vals.iter().map(|v| shape_of(v)).collect();
    let mut out_shape = (*shapes[0]).clone();
    for s in &shapes[1..] {
        out_shape = broadcast_shape(&out_shape, s)?;
    }
    let total: usize = out_shape.iter().product();
    let strides: Vec<Vec<usize>> = shapes.iter().map(|s| broadcast_strides(s, &out_shape)).collect();
    let ndim = out_shape.len();
    let last = ndim - 1;
    let row_len = out_shape[last];
    // Fill the output one last-axis run at a time, in parallel over runs and
    // vectorized within each run. A leaf varies along the last axis (stride 1,
    // read contiguously) or is constant over it (stride 0, splat).
    let contig: Vec<bool> = strides.iter().map(|s| s[last] == 1).collect();
    let fill_row = |row: usize, dst: &mut [f64]| {
        // this run's position along the outer dimensions (all but the last)
        let mut bases = vec![0usize; leaves.len()];
        let mut rem = row;
        for d in (0..last).rev() {
            let i = rem % out_shape[d];
            rem /= out_shape[d];
            for (k, s) in strides.iter().enumerate() {
                bases[k] += i * s[d];
            }
        }
        let full = row_len / 4;
        for cj in 0..full {
            let r = run_row_simd(&program, &leaves, &scalars, &bases, &contig, cj * 4).to_array();
            dst[cj * 4..cj * 4 + 4].copy_from_slice(&r);
        }
        for j in (full * 4)..row_len {
            dst[j] = run_row(&program, &leaves, &scalars, &bases, &contig, j);
        }
    };
    let mut data = vec![0.0f64; total];
    let chunk = row_len.max(1);
    #[cfg(feature = "native")]
    data.par_chunks_mut(chunk).enumerate().for_each(|(row, dst)| fill_row(row, dst));
    #[cfg(not(feature = "native"))]
    data.chunks_mut(chunk).enumerate().for_each(|(row, dst)| fill_row(row, dst));
    Ok(grid_val_shaped(data, out_shape))
}

const OVERFLOW: &str = "number too large: the result exceeds the integer limits";

/// A float that must become an int (floor, ceiling, truncate, to_int).
fn to_int_checked(r: f64) -> Result<Value, String> {
    if !r.is_finite() || !(-9.2e18..=9.2e18).contains(&r) {
        return Err(OVERFLOW.to_string());
    }
    Ok(Value::Int(r as i64))
}

fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (x, y) => real(x).partial_cmp(&real(y)).unwrap_or(Ordering::Equal),
    }
}

fn same(a: &Value, b: &Value) -> bool {
    compare(a, b) == Ordering::Equal
}

/// Every element of a collection must have a value before it can be compared.
fn no_empty_elements(items: &[Value], name: &str) -> Result<(), String> {
    if items.iter().any(|v| matches!(v, Value::Null)) {
        return Err(format!("{}: the array has elements without a value", name));
    }
    Ok(())
}

/// Position (in characters, from 1) of the first `part` in `s`, 0 if absent.
fn find(s: &str, part: &str) -> i64 {
    s.find(part).map_or(0, |byte| s[..byte].chars().count() as i64 + 1)
}

fn degrees(r: f64) -> f64 {
    r.to_degrees()
}

/// Runs a standard library function; procedures return `Value::Null`.
fn e_out_bounce(t: f64) -> f64 {
    let (n1, d1) = (7.5625, 2.75);
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

fn fact_f(n: i64) -> f64 {
    let mut r = 1.0;
    for i in 2..=n {
        r *= i as f64;
    }
    r
}

fn ncr_f(n: i64, k: i64) -> f64 {
    if k < 0 || k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    let mut r = 1.0;
    for i in 0..k {
        r = r * (n - i) as f64 / (i + 1) as f64;
    }
    r
}

fn hex_rgb(s: &str) -> (f64, f64, f64) {
    let h = s.trim().trim_start_matches('#');
    let v = |a: usize, b: usize| u8::from_str_radix(h.get(a..b).unwrap_or("0"), 16).unwrap_or(0) as f64;
    if h.len() >= 6 {
        (v(0, 2), v(2, 4), v(4, 6))
    } else if h.len() == 3 {
        let d = |c: &str| u8::from_str_radix(&format!("{c}{c}"), 16).unwrap_or(0) as f64;
        (d(&h[0..1]), d(&h[1..2]), d(&h[2..3]))
    } else {
        (0.0, 0.0, 0.0)
    }
}

fn rgb_hex(r: f64, g: f64, b: f64) -> String {
    let c = |x: f64| x.round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d.abs() < 1e-9 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s <= 0.0 {
        let v = l * 255.0;
        return (v, v, v);
    }
    let h = h.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (hue(h + 1.0 / 3.0) * 255.0, hue(h) * 255.0, hue(h - 1.0 / 3.0) * 255.0)
}

fn luminance(r: f64, g: f64, b: f64) -> f64 {
    (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
}

/// Robert Penner easing functions, `t` in 0..1.
fn easing(b: Builtin, t: f64) -> f64 {
    use Builtin::*;
    let pi = std::f64::consts::PI;
    let tau = std::f64::consts::TAU;
    match b {
        EaseInQuad => t * t,
        EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
        EaseInOutQuad => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
        EaseInCubic => t * t * t,
        EaseOutCubic => 1.0 - (1.0 - t).powi(3),
        EaseInOutCubic => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        EaseInQuart => t.powi(4),
        EaseOutQuart => 1.0 - (1.0 - t).powi(4),
        EaseInOutQuart => {
            if t < 0.5 {
                8.0 * t.powi(4)
            } else {
                1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
            }
        }
        EaseInQuint => t.powi(5),
        EaseOutQuint => 1.0 - (1.0 - t).powi(5),
        EaseInOutQuint => {
            if t < 0.5 {
                16.0 * t.powi(5)
            } else {
                1.0 - (-2.0 * t + 2.0).powi(5) / 2.0
            }
        }
        EaseInSine => 1.0 - (t * pi / 2.0).cos(),
        EaseOutSine => (t * pi / 2.0).sin(),
        EaseInOutSine => -((pi * t).cos() - 1.0) / 2.0,
        EaseInExpo => {
            if t == 0.0 {
                0.0
            } else {
                2f64.powf(10.0 * t - 10.0)
            }
        }
        EaseOutExpo => {
            if t == 1.0 {
                1.0
            } else {
                1.0 - 2f64.powf(-10.0 * t)
            }
        }
        EaseInOutExpo => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else if t < 0.5 {
                2f64.powf(20.0 * t - 10.0) / 2.0
            } else {
                (2.0 - 2f64.powf(-20.0 * t + 10.0)) / 2.0
            }
        }
        EaseInCirc => 1.0 - (1.0 - t * t).sqrt(),
        EaseOutCirc => (1.0 - (t - 1.0) * (t - 1.0)).sqrt(),
        EaseInOutCirc => {
            if t < 0.5 {
                (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0
            } else {
                ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0
            }
        }
        EaseInBack => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            c3 * t * t * t - c1 * t * t
        }
        EaseOutBack => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
        }
        EaseInOutBack => {
            let c1 = 1.70158;
            let c2 = c1 * 1.525;
            if t < 0.5 {
                ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
            } else {
                ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
            }
        }
        EaseInElastic => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                let c4 = tau / 3.0;
                -(2f64.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin()
            }
        }
        EaseOutElastic => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                let c4 = tau / 3.0;
                2f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
            }
        }
        EaseInOutElastic => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                let c5 = tau / 4.5;
                if t < 0.5 {
                    -(2f64.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0
                } else {
                    (2f64.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0 + 1.0
                }
            }
        }
        EaseInBounce => 1.0 - e_out_bounce(1.0 - t),
        EaseOutBounce => e_out_bounce(t),
        EaseInOutBounce => {
            if t < 0.5 {
                (1.0 - e_out_bounce(1.0 - 2.0 * t)) / 2.0
            } else {
                (1.0 + e_out_bounce(2.0 * t - 1.0)) / 2.0
            }
        }
        _ => t,
    }
}

/// Runtime permissions for the dangerous builtins (filesystem, network, running
/// programs). Secure-by-default is the `gw` binary's policy: it calls `set_caps`
/// with everything denied unless the matching `--allow-*` flag is given. When
/// `set_caps` is never called -- library embedding and the test suite -- every
/// capability is allowed, so nothing that worked before changes.
/// One permission: not granted, granted for anything, or granted only for a list
/// of hosts (net) or path prefixes (read/write).
#[cfg(feature = "native")]
#[derive(Clone)]
enum Allow {
    No,
    All,
    Only(Vec<String>),
}
#[cfg(feature = "native")]
impl Allow {
    fn granted(&self) -> bool {
        !matches!(self, Allow::No)
    }
}
/// Extends a permission with another `--allow-X[=scope]` flag.
#[cfg(feature = "native")]
fn grant(cur: &Allow, scope: Option<&str>) -> Allow {
    match scope {
        None => Allow::All, // a bare --allow-X grants everything
        Some(s) => {
            let items: Vec<String> =
                s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect();
            match cur {
                Allow::All => Allow::All,
                Allow::No => Allow::Only(items),
                Allow::Only(v) => {
                    let mut v = v.clone();
                    v.extend(items);
                    Allow::Only(v)
                }
            }
        }
    }
}
#[cfg(feature = "native")]
#[derive(Clone)]
pub struct Caps {
    run: bool,
    net: Allow,
    read: Allow,
    write: Allow,
}
#[cfg(feature = "native")]
impl Caps {
    pub fn all() -> Self {
        Self { run: true, net: Allow::All, read: Allow::All, write: Allow::All }
    }
    pub fn none() -> Self {
        Self { run: false, net: Allow::No, read: Allow::No, write: Allow::No }
    }
    pub fn allow_run(&mut self) {
        self.run = true;
    }
    pub fn allow_net(&mut self, scope: Option<&str>) {
        self.net = grant(&self.net, scope);
    }
    pub fn allow_read(&mut self, scope: Option<&str>) {
        self.read = grant(&self.read, scope);
    }
    pub fn allow_write(&mut self, scope: Option<&str>) {
        self.write = grant(&self.write, scope);
    }
    pub fn allow_all(&mut self) {
        *self = Self::all();
    }
}
#[cfg(feature = "native")]
static CAPS: std::sync::OnceLock<Caps> = std::sync::OnceLock::new();
/// Set the permissions once, before a program runs (a no-op if already set).
#[cfg(feature = "native")]
pub fn set_caps(c: Caps) {
    let _ = CAPS.set(c);
}
#[cfg(feature = "native")]
fn caps() -> &'static Caps {
    CAPS.get_or_init(Caps::all)
}
#[cfg(feature = "native")]
fn deny(what: &str, flag: &str) -> String {
    format!("{what} is not permitted \u{2014} run gw with {flag}")
}
#[cfg(feature = "native")]
fn path_norm(p: &str) -> String {
    p.replace('\\', "/")
}
/// Checks a filesystem path against a read/write permission (prefix match).
#[cfg(feature = "native")]
fn check_path(a: &Allow, path: &str, what: &str, flag: &str) -> Result<(), String> {
    match a {
        Allow::No => Err(deny(what, flag)),
        Allow::All => Ok(()),
        Allow::Only(prefixes) => {
            let p = path_norm(path);
            let ok = prefixes.iter().any(|pre| {
                let pre = path_norm(pre);
                let pre = pre.trim_end_matches('/');
                p == pre || p.starts_with(&format!("{pre}/"))
            });
            if ok {
                Ok(())
            } else {
                Err(format!("{what} at \"{path}\" is not permitted \u{2014} run gw with {flag}=<path>"))
            }
        }
    }
}
/// Checks a URL's host against a net permission.
#[cfg(feature = "native")]
fn check_host(a: &Allow, url: &str, what: &str) -> Result<(), String> {
    match a {
        Allow::No => Err(deny(what, "--allow-net")),
        Allow::All => Ok(()),
        Allow::Only(hosts) => {
            let h = url_host(url);
            if hosts.iter().any(|x| x.eq_ignore_ascii_case(&h)) {
                Ok(())
            } else {
                Err(format!("{what} to \"{h}\" is not permitted \u{2014} run gw with --allow-net={h}"))
            }
        }
    }
}
/// Refuses a dangerous builtin whose capability was not granted, or whose target
/// host/path falls outside the granted scope.
#[cfg(feature = "native")]
fn cap_check(b: Builtin, args: &[Value]) -> Result<(), String> {
    use Builtin::*;
    let c = caps();
    match b {
        ShellRun => {
            if !c.run {
                return Err(deny("shell.run", "--allow-run"));
            }
        }
        ProcStart | ProcDone | ProcWait | ProcKill => {
            if !c.run {
                return Err(deny("the proc module", "--allow-run"));
            }
        }
        FfiOpen | FfiInt | FfiInt32 | FfiReal | FfiCstr | FfiRead | FfiBuffer | FfiCallInt | FfiCallReal
        | FfiAlloc | FfiPokeInt | FfiPokeI32 | FfiPokeReal | FfiPeekInt | FfiPeekI32 | FfiPeekReal => {
            if !c.run {
                return Err(deny("the ffi module", "--allow-run"));
            }
        }
        WebGet | WebPost | WebStatus | WebDownload => check_host(&c.net, text(&args[0]), "the web module")?,
        ServeStart | ServeWait | ServeReply | ServeStop => {
            if !c.net.granted() {
                return Err(deny("the serve module", "--allow-net"));
            }
        }
        NetRequest | NetSend | NetStatus | NetHeader | NetOk => {
            if !c.net.granted() {
                return Err(deny("the net module", "--allow-net"));
            }
        }
        AiAsk | AiSay => {
            if !c.net.granted() {
                return Err(deny("ai requests", "--allow-net"));
            }
        }
        WriteFile => check_path(&c.write, text(&args[0]), "writing files", "--allow-write")?,
        ChartLine | ChartBars | ChartScatter => {
            if let Some(p) = args.last() {
                check_path(&c.write, text(p), "writing files", "--allow-write")?;
            }
        }
        DbRun => {
            if !c.write.granted() {
                return Err(deny("db.run", "--allow-write"));
            }
        }
        DbOpen => {
            let p = text(&args[0]);
            if p == ":memory:" {
                if !c.read.granted() {
                    return Err(deny("db.open", "--allow-read"));
                }
            } else {
                check_path(&c.read, p, "opening a database", "--allow-read")?;
            }
        }
        ReadFile => check_path(&c.read, text(&args[0]), "reading files", "--allow-read")?,
        DbValue | DbQuery => {
            if !c.read.granted() {
                return Err(deny("the db module", "--allow-read"));
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn call(b: Builtin, args: &[Value]) -> Result<Value, String> {
    use Builtin::*;
    #[cfg(feature = "native")]
    cap_check(b, args)?;
    // A GPU-resident array reaching a builtin that cannot run on the GPU is
    // brought back to the CPU first (so it works instead of panicking).
    #[cfg(feature = "gpu")]
    let _dl;
    #[cfg(feature = "gpu")]
    let args: &[Value] = if !gpu_resident_aware(b) && args.iter().any(|v| matches!(v, Value::GpuFloats(..))) {
        _dl = gpu_download_args(args)?;
        &_dl
    } else {
        args
    };
    Ok(match b {
        // ======== batch H1 ========
        EaseInQuad | EaseOutQuad | EaseInOutQuad | EaseInCubic | EaseOutCubic | EaseInOutCubic
        | EaseInQuart | EaseOutQuart | EaseInOutQuart | EaseInQuint | EaseOutQuint | EaseInOutQuint
        | EaseInSine | EaseOutSine | EaseInOutSine | EaseInExpo | EaseOutExpo | EaseInOutExpo
        | EaseInCirc | EaseOutCirc | EaseInOutCirc | EaseInBack | EaseOutBack | EaseInOutBack
        | EaseInElastic | EaseOutElastic | EaseInOutElastic | EaseInBounce | EaseOutBounce
        | EaseInOutBounce => Value::Real(easing(b, real(&args[0]))),
        TrigHaversin => Value::Real((real(&args[0]).to_radians() / 2.0).sin().powi(2)),
        TrigVersin => Value::Real(1.0 - real(&args[0]).to_radians().cos()),
        TrigCoversin => Value::Real(1.0 - real(&args[0]).to_radians().sin()),
        TrigExsec => Value::Real(1.0 / real(&args[0]).to_radians().cos() - 1.0),
        TrigExcosec => Value::Real(1.0 / real(&args[0]).to_radians().sin() - 1.0),
        TrigCrd => Value::Real(2.0 * (real(&args[0]).to_radians() / 2.0).sin()),
        SpecialStirling => {
            let n = real(&args[0]);
            Value::Real((2.0 * std::f64::consts::PI * n).sqrt() * (n / std::f64::consts::E).powf(n))
        }
        SpecialLogit => {
            let p = real(&args[0]);
            Value::Real((p / (1.0 - p)).ln())
        }
        SpecialSoftplus => Value::Real((1.0 + real(&args[0]).exp()).ln()),
        SpecialGudermann => Value::Real(2.0 * real(&args[0]).exp().atan() - std::f64::consts::FRAC_PI_2),
        PolarX => Value::Real(real(&args[0]) * real(&args[1]).to_radians().cos()),
        PolarY => Value::Real(real(&args[0]) * real(&args[1]).to_radians().sin()),
        PolarR => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        PolarAngle => Value::Real(real(&args[1]).atan2(real(&args[0])).to_degrees()),
        TransformRotX => {
            let (x, y, a) = (real(&args[0]), real(&args[1]), real(&args[2]).to_radians());
            Value::Real(x * a.cos() - y * a.sin())
        }
        TransformRotY => {
            let (x, y, a) = (real(&args[0]), real(&args[1]), real(&args[2]).to_radians());
            Value::Real(x * a.sin() + y * a.cos())
        }
        Temp2CToRankine => Value::Real((real(&args[0]) + 273.15) * 9.0 / 5.0),
        Temp2RankineToC => Value::Real(real(&args[0]) * 5.0 / 9.0 - 273.15),
        Temp2FToRankine => Value::Real(real(&args[0]) + 459.67),
        Temp2RankineToF => Value::Real(real(&args[0]) - 459.67),
        Temp2CToReaumur => Value::Real(real(&args[0]) * 4.0 / 5.0),
        Temp2ReaumurToC => Value::Real(real(&args[0]) * 5.0 / 4.0),
        StorageBytesToKb | StorageKbToMb | StorageMbToGb | StorageGbToTb | StorageTbToPb => {
            Value::Real(real(&args[0]) / 1024.0)
        }
        StorageBitsToBytes => Value::Real(real(&args[0]) / 8.0),
        Area2AcresToHa => Value::Real(real(&args[0]) * 0.404686),
        Area2HaToAcres => Value::Real(real(&args[0]) / 0.404686),
        Area2SqkmToSqmi => Value::Real(real(&args[0]) * 0.386102),
        Area2SqmiToSqkm => Value::Real(real(&args[0]) / 0.386102),
        KitchenTspToTbsp => Value::Real(real(&args[0]) / 3.0),
        KitchenTbspToCup => Value::Real(real(&args[0]) / 16.0),
        KitchenCupToFloz => Value::Real(real(&args[0]) * 8.0),
        KitchenFlozToMl => Value::Real(real(&args[0]) * 29.5735),
        KitchenMlToFloz => Value::Real(real(&args[0]) / 29.5735),
        KitchenOzToGrams => Value::Real(real(&args[0]) * 28.3495),
        KitchenGramsToOz => Value::Real(real(&args[0]) / 28.3495),
        KitchenLbToOz => Value::Real(real(&args[0]) * 16.0),

        // ======== batch H2 ========
        ChemMolarity => Value::Real(real(&args[0]) / real(&args[1])),
        ChemMoles => Value::Real(real(&args[0]) / real(&args[1])),
        ChemMass => Value::Real(real(&args[0]) * real(&args[1])),
        ChemDilution => Value::Real(real(&args[0]) * real(&args[1]) / real(&args[2])),
        ChemPoh => Value::Real(14.0 - real(&args[0])),
        ChemPercentYield => Value::Real(real(&args[0]) / real(&args[1]) * 100.0),
        ChemGasPressure => Value::Real(real(&args[0]) * 0.082057 * real(&args[1]) / real(&args[2])),
        ChemDensity => Value::Real(real(&args[0]) / real(&args[1])),
        ChemAtoms => Value::Real(real(&args[0]) * 6.02214076e23),
        ChemMolesFromAtoms => Value::Real(real(&args[0]) / 6.02214076e23),
        ElecPower => Value::Real(real(&args[0]) * real(&args[1])),
        ElecEnergy => Value::Real(real(&args[0]) * real(&args[1])),
        ElecChargeCap => Value::Real(real(&args[0]) * real(&args[1])),
        ElecInductorEnergy => Value::Real(0.5 * real(&args[0]) * real(&args[1]).powi(2)),
        ElecRcTime => Value::Real(real(&args[0]) * real(&args[1])),
        ElecReactanceC => Value::Real(1.0 / (2.0 * std::f64::consts::PI * real(&args[0]) * real(&args[1]))),
        ElecReactanceL => Value::Real(2.0 * std::f64::consts::PI * real(&args[0]) * real(&args[1])),
        ElecResonance => Value::Real(1.0 / (2.0 * std::f64::consts::PI * (real(&args[0]) * real(&args[1])).sqrt())),
        Fin2Cagr => {
            let (b0, e, y) = (real(&args[0]), real(&args[1]), int(&args[2]) as f64);
            Value::Real((e / b0).powf(1.0 / y) - 1.0)
        }
        Fin2Pv => {
            let (fv, r, y) = (real(&args[0]), real(&args[1]), int(&args[2]) as i32);
            Value::Real(fv / (1.0 + r / 100.0).powi(y))
        }
        Fin2Fv => {
            let (pv, r, y) = (real(&args[0]), real(&args[1]), int(&args[2]) as i32);
            Value::Real(pv * (1.0 + r / 100.0).powi(y))
        }
        Fin2EffectiveRate => {
            let (nom, p) = (real(&args[0]), int(&args[1]) as f64);
            Value::Real(((1.0 + nom / 100.0 / p).powf(p) - 1.0) * 100.0)
        }
        Fin2RealReturn => {
            let (nom, inf) = (real(&args[0]), real(&args[1]));
            Value::Real(((1.0 + nom / 100.0) / (1.0 + inf / 100.0) - 1.0) * 100.0)
        }
        Fin2DebtRatio => Value::Real(real(&args[0]) / real(&args[1])),
        Fin2CurrentRatio => Value::Real(real(&args[0]) / real(&args[1])),
        Fin2Roe => Value::Real(real(&args[0]) / real(&args[1]) * 100.0),
        Fin2Eps => Value::Real(real(&args[0]) / real(&args[1])),
        Fin2PeRatio => Value::Real(real(&args[0]) / real(&args[1])),
        H2Bsa => Value::Real((real(&args[0]) * real(&args[1]) / 3600.0).sqrt()),
        H2WaistHip => Value::Real(real(&args[0]) / real(&args[1])),
        H2TargetHr => Value::Real((220.0 - real(&args[0])) * real(&args[1]) / 100.0),
        H2CaloriesWalk => Value::Real(real(&args[0]) * 0.0563 * real(&args[1])),
        H2CaloriesRun => Value::Real(real(&args[0]) * 0.12 * real(&args[1])),
        H2Vo2max => Value::Real(15.3 * (real(&args[0]) / real(&args[1]))),
        MusicSemitonesBetween => Value::Real(12.0 * (real(&args[1]) / real(&args[0])).log2()),
        MusicTranspose => Value::Real(real(&args[0]) * 2f64.powf(real(&args[1]) / 12.0)),
        MusicBpmToHz => Value::Real(real(&args[0]) / 60.0),
        MusicHzToBpm => Value::Real(real(&args[0]) * 60.0),
        MusicCentsToRatio => Value::Real(2f64.powf(real(&args[0]) / 1200.0)),
        MusicRatioToCents => Value::Real(1200.0 * real(&args[0]).log2()),
        SpaceGravForce => {
            let (m1, m2, r) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(6.674e-11 * m1 * m2 / (r * r))
        }
        SpaceSchwarzschild => Value::Real(2.0 * 6.674e-11 * real(&args[0]) / 299792458.0_f64.powi(2)),
        SpaceOrbitalVel => Value::Real((6.674e-11 * real(&args[0]) / real(&args[1])).sqrt()),
        SpaceSurfaceGravity => {
            let (m, r) = (real(&args[0]), real(&args[1]));
            Value::Real(6.674e-11 * m / (r * r))
        }
        OpticsMagnification => Value::Real(-real(&args[0]) / real(&args[1])),
        OpticsFocalRadius => Value::Real(real(&args[0]) / 2.0),
        OpticsLensPower => Value::Real(1.0 / real(&args[0])),
        OpticsSnell => {
            let (n1, a1, n2) = (real(&args[0]), real(&args[1]).to_radians(), real(&args[2]));
            Value::Real((n1 * a1.sin() / n2).asin().to_degrees())
        }
        Prob2Binomial => {
            let (n, k, p) = (int(&args[0]), int(&args[1]), real(&args[2]));
            Value::Real(ncr_f(n, k) * p.powi(k as i32) * (1.0 - p).powi((n - k) as i32))
        }
        Prob2Poisson => {
            let (k, l) = (int(&args[0]), real(&args[1]));
            Value::Real((-l).exp() * l.powi(k as i32) / fact_f(k))
        }
        Prob2NormalCdf => Value::Real(0.5 * (1.0 + erf(real(&args[0]) / std::f64::consts::SQRT_2))),
        Prob2NormalPdf => {
            let x = real(&args[0]);
            Value::Real((-x * x / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt())
        }
        RpgDamage => Value::Real((real(&args[0]) - real(&args[1])).max(1.0)),
        RpgXpForLevel => {
            let l = int(&args[0]);
            Value::Int(l * l * 100)
        }
        RpgLevelFromXp => {
            let xp = int(&args[0]);
            Value::Int((xp as f64 / 100.0).sqrt().floor() as i64 + 1)
        }
        RpgCrit => Value::Real(real(&args[0]) * real(&args[1])),
        RpgHitChance => Value::Real((real(&args[0]) - real(&args[1])).clamp(0.0, 100.0)),
        Phys2EnergyMass => Value::Real(real(&args[0]) * 299792458.0_f64.powi(2)),
        Phys2PhotonEnergy => Value::Real(6.62607015e-34 * real(&args[0])),
        Phys2DeBroglie => Value::Real(6.62607015e-34 / real(&args[0])),
        Phys2Coulomb => {
            let (q1, q2, r) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(8.9875517873681764e9 * q1 * q2 / (r * r))
        }
        Stats2Cv => Value::Real(real(&args[1]) / real(&args[0]) * 100.0),
        Stats2Se => Value::Real(real(&args[0]) / (int(&args[1]) as f64).sqrt()),
        Stats2Margin => Value::Real(real(&args[2]) * real(&args[0]) / (int(&args[1]) as f64).sqrt()),
        MapMercatorY => {
            let lat = real(&args[0]).to_radians();
            Value::Real((std::f64::consts::FRAC_PI_4 + lat / 2.0).tan().ln())
        }
        MapTileX => {
            let (lon, z) = (real(&args[0]), int(&args[1]));
            let n = 2f64.powi(z as i32);
            Value::Int(((lon + 180.0) / 360.0 * n).floor() as i64)
        }
        MapTileY => {
            let (lat, z) = (real(&args[0]).to_radians(), int(&args[1]));
            let n = 2f64.powi(z as i32);
            Value::Int(((1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n).floor() as i64)
        }
        Date2DayOfYear => {
            let (y, m, d) = (int(&args[0]), int(&args[1]), int(&args[2]));
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
            let mut total = d;
            for i in 0..(m.clamp(1, 12) - 1) as usize {
                total += days[i];
            }
            Value::Int(total)
        }
        Date2Zeller => {
            let (mut y, mut m, d) = (int(&args[0]), int(&args[1]), int(&args[2]));
            if m < 3 {
                m += 12;
                y -= 1;
            }
            let (k, j) = (y % 100, y / 100);
            Value::Int((d + 13 * (m + 1) / 5 + k + k / 4 + j / 4 + 5 * j).rem_euclid(7))
        }
        Date2DaysInYear => {
            let y = int(&args[0]);
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            Value::Int(if leap { 366 } else { 365 })
        }
        Wave2Angular => Value::Real(2.0 * std::f64::consts::PI * real(&args[0])),
        Wave2Period => Value::Real(1.0 / real(&args[0])),
        Wave2Frequency => Value::Real(1.0 / real(&args[0])),
        LightSpeed => Value::Real(299792.458),
        LightTimeToTravel => Value::Real(real(&args[0]) / 299792.458),
        LightDistanceLy => Value::Real(real(&args[0]) / 9.4607304725808e12),
        LightYearsToKm => Value::Real(real(&args[0]) * 9.4607304725808e12),
        Sound2Speed => Value::Real(343.0),
        Sound2DistFromDelay => Value::Real(real(&args[0]) * 343.0),
        Sound2TimeForDist => Value::Real(real(&args[0]) / 343.0),

        // ======== gtq batch G: pure colour helpers ========
        GtqLighten | GtqTint => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let a = real(&args[1]);
            string(rgb_hex(r + (255.0 - r) * a, g + (255.0 - g) * a, b + (255.0 - b) * a))
        }
        GtqDarken | GtqShade => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let a = real(&args[1]);
            string(rgb_hex(r * (1.0 - a), g * (1.0 - a), b * (1.0 - a)))
        }
        GtqMix => {
            let (r1, g1, b1) = hex_rgb(text(&args[0]));
            let (r2, g2, b2) = hex_rgb(text(&args[1]));
            let t = real(&args[2]);
            string(rgb_hex(r1 + (r2 - r1) * t, g1 + (g2 - g1) * t, b1 + (b2 - b1) * t))
        }
        GtqGrayscale => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            string(rgb_hex(y, y, y))
        }
        GtqInvert => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            string(rgb_hex(255.0 - r, 255.0 - g, 255.0 - b))
        }
        GtqComplement => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (r, g, b) = hsl_to_rgb(h + 180.0, s, l);
            string(rgb_hex(r, g, b))
        }
        GtqContrastColor => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            string(if luminance(r, g, b) > 0.5 { "#000000" } else { "#ffffff" }.to_string())
        }
        GtqFromRgb => string(rgb_hex(int(&args[0]) as f64, int(&args[1]) as f64, int(&args[2]) as f64)),
        GtqFromHsl => {
            let (r, g, b) = hsl_to_rgb(real(&args[0]), real(&args[1]), real(&args[2]));
            string(rgb_hex(r, g, b))
        }
        GtqHueShift => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (r, g, b) = hsl_to_rgb(h + real(&args[1]), s, l);
            string(rgb_hex(r, g, b))
        }
        GtqSaturate => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (r, g, b) = hsl_to_rgb(h, (s + real(&args[1])).clamp(0.0, 1.0), l);
            string(rgb_hex(r, g, b))
        }
        GtqDesaturate | GtqMute => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (r, g, b) = hsl_to_rgb(h, (s - real(&args[1])).clamp(0.0, 1.0), l);
            string(rgb_hex(r, g, b))
        }
        GtqWithRed => {
            let (_, g, b) = hex_rgb(text(&args[0]));
            string(rgb_hex(int(&args[1]) as f64, g, b))
        }
        GtqWithGreen => {
            let (r, _, b) = hex_rgb(text(&args[0]));
            string(rgb_hex(r, int(&args[1]) as f64, b))
        }
        GtqWithBlue => {
            let (r, g, _) = hex_rgb(text(&args[0]));
            string(rgb_hex(r, g, int(&args[1]) as f64))
        }
        GtqBrighten => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let f = 1.0 + real(&args[1]);
            string(rgb_hex(r * f, g * f, b * f))
        }
        GtqDim => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            let f = 1.0 - real(&args[1]);
            string(rgb_hex(r * f, g * f, b * f))
        }
        GtqRedOf => Value::Int(hex_rgb(text(&args[0])).0 as i64),
        GtqGreenOf => Value::Int(hex_rgb(text(&args[0])).1 as i64),
        GtqBlueOf => Value::Int(hex_rgb(text(&args[0])).2 as i64),
        GtqLuminanceOf => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            Value::Real(luminance(r, g, b))
        }
        GtqIsDark => {
            let (r, g, b) = hex_rgb(text(&args[0]));
            Value::Bool(luminance(r, g, b) < 0.5)
        }

        // ======== gtq batch G: pure layout helpers ========
        GtqRowX | GtqColY | GtqGridX | GtqGridY => {
            Value::Real(real(&args[0]) + int(&args[1]) as f64 * (real(&args[2]) + real(&args[3])))
        }
        GtqCenterX | GtqCenterY => Value::Real(real(&args[0]) + (real(&args[1]) - real(&args[2])) / 2.0),
        GtqRightX | GtqBottomY => {
            Value::Real(real(&args[0]) + real(&args[1]) - real(&args[2]) - real(&args[3]))
        }
        GtqFitW | GtqFitH => {
            let (w, n, gap) = (real(&args[0]), int(&args[1]) as f64, real(&args[2]));
            Value::Real((w - (n - 1.0) * gap) / n)
        }
        GtqAspectW => Value::Real(real(&args[0]) * real(&args[1])),
        GtqAspectH => Value::Real(real(&args[0]) / real(&args[1])),
        GtqLerpPos => Value::Real(real(&args[0]) + (real(&args[1]) - real(&args[0])) * real(&args[2])),
        GtqRemap => {
            let (v, a, b, c, d) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
            Value::Real(c + (v - a) / (b - a) * (d - c))
        }
        GtqSnap => {
            let (v, grid) = (real(&args[0]), real(&args[1]));
            Value::Real((v / grid).round() * grid)
        }
        GtqGolden => Value::Real(real(&args[0]) * 0.618_033_988_749_895),
        GtqPadIn => Value::Real(real(&args[0]) + real(&args[1])),
        GtqInsetW => Value::Real(real(&args[0]) - 2.0 * real(&args[1])),
        GtqDistribute => {
            let (span, n, i) = (real(&args[0]), int(&args[1]) as f64, int(&args[2]) as f64);
            Value::Real((i + 0.5) * span / n)
        }
        GtqSpanEnd => {
            let (s, n, iw, gap) = (real(&args[0]), int(&args[1]) as f64, real(&args[2]), real(&args[3]));
            Value::Real(s + n * (iw + gap) - gap)
        }
        GtqScaleAt => {
            let (v, o, f) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(o + (v - o) * f)
        }
        GtqWrapIndex => Value::Int(int(&args[0]).rem_euclid(int(&args[1]).max(1))),
        GtqClampPos => Value::Real(real(&args[0]).clamp(real(&args[1]), real(&args[2]))),
        GtqCellX | GtqCellY => Value::Real(real(&args[0]) + int(&args[1]) as f64 * real(&args[2])),

        // ======== module "net": rich HTTP client ========
        #[cfg(not(feature = "native"))]
        NetRequest | NetSend | NetStatus | NetHeader | NetOk => return Err(NET_OFF.to_string()),
        #[cfg(feature = "native")]
        NetRequest => {
            let (m, u, b) = (text(&args[0]), text(&args[1]), text(&args[2]));
            let resp = net_build(m, u, b, None).map_err(|e| web_error("net.request", u, e))?;
            string(resp.into_string().map_err(|e| format!("net.request: cannot read the reply: {}", e))?)
        }
        #[cfg(feature = "native")]
        NetSend => {
            let (m, u, b, hn, hv) =
                (text(&args[0]), text(&args[1]), text(&args[2]), text(&args[3]), text(&args[4]));
            let resp = net_build(m, u, b, Some((hn, hv))).map_err(|e| web_error("net.send", u, e))?;
            string(resp.into_string().map_err(|e| format!("net.send: cannot read the reply: {}", e))?)
        }
        #[cfg(feature = "native")]
        NetStatus => {
            let (m, u) = (text(&args[0]), text(&args[1]));
            match net_build(m, u, "", None) {
                Ok(r) => Value::Int(r.status() as i64),
                Err(ureq::Error::Status(code, _)) => Value::Int(code as i64),
                Err(ureq::Error::Transport(t)) => return Err(format!("net.status: cannot reach \"{}\": {}", u, t)),
            }
        }
        #[cfg(feature = "native")]
        NetHeader => {
            let (u, name) = (text(&args[0]), text(&args[1]));
            let resp = net_build("GET", u, "", None).map_err(|e| web_error("net.header", u, e))?;
            string(resp.header(name).unwrap_or("").to_string())
        }
        #[cfg(feature = "native")]
        NetOk => {
            let u = text(&args[0]);
            match net_build("GET", u, "", None) {
                Ok(r) => Value::Bool((200..300).contains(&r.status())),
                Err(ureq::Error::Status(_, _)) => Value::Bool(false),
                Err(ureq::Error::Transport(_)) => Value::Bool(false),
            }
        }
        NetHost => string(url_host(text(&args[0]))),
        NetPath => string(url_path(text(&args[0]))),
        NetQuery => string(url_query(text(&args[0]), text(&args[1]))),

        // ======== unicode-aware string operations ========
        UnicodeUpper => string(text(&args[0]).to_uppercase()),
        UnicodeLower => string(text(&args[0]).to_lowercase()),
        UnicodeReverse => string(text(&args[0]).chars().rev().collect::<String>()),
        UnicodeTrim => string(text(&args[0]).trim().to_string()),
        UnicodeIsLetter => Value::Bool(text(&args[0]).chars().next().is_some_and(|c| c.is_alphabetic())),
        UnicodeIsDigit => Value::Bool(text(&args[0]).chars().next().is_some_and(|c| c.is_numeric())),
        UnicodeIsSpace => Value::Bool(text(&args[0]).chars().next().is_some_and(|c| c.is_whitespace())),
        UnicodeIsUpper => Value::Bool(text(&args[0]).chars().next().is_some_and(|c| c.is_uppercase())),

        // ======== module "gwgame": 2D game toolkit ========
        GwgAabb => {
            let (ax, ay, aw, ah) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
            let (bx, by, bw, bh) = (real(&args[4]), real(&args[5]), real(&args[6]), real(&args[7]));
            Value::Bool(ax < bx + bw && ax + aw > bx && ay < by + bh && ay + ah > by)
        }
        GwgHitCircle => {
            let (ax, ay, ar, bx, by, br) =
                (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]), real(&args[5]));
            Value::Bool((ax - bx).hypot(ay - by) <= ar + br)
        }
        GwgPointIn => {
            let (px, py, x, y, w, h) =
                (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]), real(&args[5]));
            Value::Bool(px >= x && px <= x + w && py >= y && py <= y + h)
        }
        GwgDistance => Value::Real((real(&args[0]) - real(&args[2])).hypot(real(&args[1]) - real(&args[3]))),
        GwgDirection => {
            let (dx, dy) = (real(&args[2]) - real(&args[0]), real(&args[3]) - real(&args[1]));
            Value::Real(dy.atan2(dx).to_degrees())
        }
        GwgMoveToward => {
            let (cur, target, step) = (real(&args[0]), real(&args[1]), real(&args[2]));
            let d = target - cur;
            Value::Real(if d.abs() <= step { target } else { cur + d.signum() * step })
        }
        GwgWrap => {
            let (v, lo, hi) = (real(&args[0]), real(&args[1]), real(&args[2]));
            let span = hi - lo;
            Value::Real(if span <= 0.0 { lo } else { lo + (v - lo).rem_euclid(span) })
        }
        GwgLerp => Value::Real(real(&args[0]) + (real(&args[1]) - real(&args[0])) * real(&args[2])),
        GwgVx => Value::Real(real(&args[0]) * real(&args[1]).to_radians().cos()),
        GwgVy => Value::Real(real(&args[0]) * real(&args[1]).to_radians().sin()),
        GwgOverlap => {
            let (a0, a1, b0, b1) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
            Value::Real((a1.min(b1) - a0.max(b0)).max(0.0))
        }
        GwgSign => Value::Int(real(&args[0]).signum() as i64 * (real(&args[0]) != 0.0) as i64),
        GwgSnap => {
            let (v, size) = (real(&args[0]), real(&args[1]));
            Value::Real(if size == 0.0 { v } else { (v / size).round() * size })
        }
        GwgCell => {
            let (v, size) = (real(&args[0]), real(&args[1]));
            Value::Int(if size == 0.0 { 0 } else { (v / size).floor() as i64 })
        }
        GwgBounce => {
            let (pos, vel, lo, hi) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
            Value::Real(if (pos <= lo && vel < 0.0) || (pos >= hi && vel > 0.0) { -vel } else { vel })
        }
        GwgClampf => Value::Real(real(&args[0]).clamp(real(&args[1]), real(&args[2]))),
        GwgAngleDiff => {
            let (a, b) = (real(&args[0]), real(&args[1]));
            Value::Real((b - a + 180.0).rem_euclid(360.0) - 180.0)
        }
        GwgReached => Value::Bool((real(&args[0]) - real(&args[1])).abs() <= real(&args[2])),

        // ======== module "grad": reverse-mode autodiff ========
        GradVar | GradConst => Value::Int(g_push(real(&args[0]), 0, 0, 0, 0.0)),
        GradAdd => {
            let (a, b) = (int(&args[0]), int(&args[1]));
            let v = g_val(a)? + g_val(b)?;
            Value::Int(g_push(v, 1, a as usize, b as usize, 0.0))
        }
        GradSub => {
            let (a, b) = (int(&args[0]), int(&args[1]));
            let v = g_val(a)? - g_val(b)?;
            Value::Int(g_push(v, 2, a as usize, b as usize, 0.0))
        }
        GradMul => {
            let (a, b) = (int(&args[0]), int(&args[1]));
            let v = g_val(a)? * g_val(b)?;
            Value::Int(g_push(v, 3, a as usize, b as usize, 0.0))
        }
        GradDiv => {
            let (a, b) = (int(&args[0]), int(&args[1]));
            let v = g_val(a)? / g_val(b)?;
            Value::Int(g_push(v, 4, a as usize, b as usize, 0.0))
        }
        GradNeg => {
            let a = int(&args[0]);
            let v = -g_val(a)?;
            Value::Int(g_push(v, 5, a as usize, 0, 0.0))
        }
        GradPow => {
            let (a, k) = (int(&args[0]), real(&args[1]));
            let v = g_val(a)?.powf(k);
            Value::Int(g_push(v, 6, a as usize, 0, k))
        }
        GradExp => {
            let a = int(&args[0]);
            let v = g_val(a)?.exp();
            Value::Int(g_push(v, 7, a as usize, 0, 0.0))
        }
        GradLn => {
            let a = int(&args[0]);
            let v = g_val(a)?.ln();
            Value::Int(g_push(v, 8, a as usize, 0, 0.0))
        }
        GradSin => {
            let a = int(&args[0]);
            let v = g_val(a)?.sin();
            Value::Int(g_push(v, 9, a as usize, 0, 0.0))
        }
        GradCos => {
            let a = int(&args[0]);
            let v = g_val(a)?.cos();
            Value::Int(g_push(v, 10, a as usize, 0, 0.0))
        }
        GradTanh => {
            let a = int(&args[0]);
            let v = g_val(a)?.tanh();
            Value::Int(g_push(v, 11, a as usize, 0, 0.0))
        }
        GradSigmoid => {
            let a = int(&args[0]);
            let v = 1.0 / (1.0 + (-g_val(a)?).exp());
            Value::Int(g_push(v, 12, a as usize, 0, 0.0))
        }
        GradRelu => {
            let a = int(&args[0]);
            let v = g_val(a)?.max(0.0);
            Value::Int(g_push(v, 13, a as usize, 0, 0.0))
        }
        GradValue => Value::Real(g_val(int(&args[0]))?),
        GradBackward => {
            g_backward(int(&args[0]))?;
            Value::Null
        }
        GradGrad => Value::Real(g_grad(int(&args[0]))?),
        GradReset => {
            GRAD.with(|g| g.borrow_mut().clear());
            Value::Null
        }
        GradCount => Value::Int(GRAD.with(|g| g.borrow().len() as i64)),

        // ======== module "ai": OpenAI-compatible chat ========
        AiUseGroq => {
            let (key, model) = (text(&args[0]).to_string(), text(&args[1]).to_string());
            AI.with(|c| {
                let mut cfg = c.borrow_mut();
                cfg.endpoint = "https://api.groq.com/openai/v1".to_string();
                cfg.key = key;
                cfg.model = model;
            });
            Value::Null
        }
        AiUseOpenRouter => {
            let (key, model) = (text(&args[0]).to_string(), text(&args[1]).to_string());
            AI.with(|c| {
                let mut cfg = c.borrow_mut();
                cfg.endpoint = "https://openrouter.ai/api/v1".to_string();
                cfg.key = key;
                cfg.model = model;
            });
            Value::Null
        }
        AiUseOpenAI => {
            let (key, model) = (text(&args[0]).to_string(), text(&args[1]).to_string());
            AI.with(|c| {
                let mut cfg = c.borrow_mut();
                cfg.endpoint = "https://api.openai.com/v1".to_string();
                cfg.key = key;
                cfg.model = model;
            });
            Value::Null
        }
        AiUseOllama => {
            let model = text(&args[0]).to_string();
            AI.with(|c| {
                let mut cfg = c.borrow_mut();
                cfg.endpoint = "http://localhost:11434/v1".to_string();
                cfg.key = String::new();
                cfg.model = model;
            });
            Value::Null
        }
        AiEndpoint => {
            let v = text(&args[0]).to_string();
            AI.with(|c| c.borrow_mut().endpoint = v);
            Value::Null
        }
        AiKey => {
            let v = text(&args[0]).to_string();
            AI.with(|c| c.borrow_mut().key = v);
            Value::Null
        }
        AiModel => {
            let v = text(&args[0]).to_string();
            AI.with(|c| c.borrow_mut().model = v);
            Value::Null
        }
        AiSystem => {
            let v = text(&args[0]).to_string();
            AI.with(|c| c.borrow_mut().system = v);
            Value::Null
        }
        AiTemperature => {
            let v = real(&args[0]);
            AI.with(|c| c.borrow_mut().temperature = v);
            Value::Null
        }
        AiAsk => {
            let prompt = text(&args[0]).to_string();
            let answer = AI.with(|c| {
                let mut cfg = c.borrow_mut();
                let (ans, tok) = ai_send(&cfg, ai_messages(&cfg, Some(&prompt)), false)?;
                cfg.last = ans.clone();
                cfg.last_tokens = tok;
                Ok::<String, String>(ans)
            })?;
            string(answer)
        }
        AiSay => {
            let prompt = text(&args[0]).to_string();
            let answer = AI.with(|c| {
                let mut cfg = c.borrow_mut();
                let messages = ai_messages(&cfg, Some(&prompt));
                let (ans, tok) = ai_send(&cfg, messages, false)?;
                cfg.history.push(("user".to_string(), prompt));
                cfg.history.push(("assistant".to_string(), ans.clone()));
                cfg.last = ans.clone();
                cfg.last_tokens = tok;
                Ok::<String, String>(ans)
            })?;
            string(answer)
        }
        AiReset => {
            AI.with(|c| c.borrow_mut().history.clear());
            Value::Null
        }
        AiMaxTokens => {
            let v = int(&args[0]);
            AI.with(|c| c.borrow_mut().max_tokens = if v > 0 { Some(v) } else { None });
            Value::Null
        }
        AiTopP => {
            let v = real(&args[0]);
            AI.with(|c| c.borrow_mut().top_p = Some(v));
            Value::Null
        }
        AiSeed => {
            let v = int(&args[0]);
            AI.with(|c| c.borrow_mut().seed = Some(v));
            Value::Null
        }
        AiStop => {
            let v = text(&args[0]).to_string();
            AI.with(|c| c.borrow_mut().stop = if v.is_empty() { None } else { Some(v) });
            Value::Null
        }
        AiTimeout => {
            let v = int(&args[0]);
            AI.with(|c| c.borrow_mut().timeout = if v > 0 { v as u64 } else { 1 });
            Value::Null
        }
        AiAddSystem => {
            let v = text(&args[0]).to_string();
            AI.with(|c| c.borrow_mut().history.push(("system".to_string(), v)));
            Value::Null
        }
        AiAddUser => {
            let v = text(&args[0]).to_string();
            AI.with(|c| c.borrow_mut().history.push(("user".to_string(), v)));
            Value::Null
        }
        AiAddAssistant => {
            let v = text(&args[0]).to_string();
            AI.with(|c| c.borrow_mut().history.push(("assistant".to_string(), v)));
            Value::Null
        }
        AiHistory => {
            let s = AI.with(|c| {
                let cfg = c.borrow();
                cfg.history.iter().map(|(r, t)| format!("{}: {}", r, t)).collect::<Vec<_>>().join("\n")
            });
            string(s)
        }
        AiLast => string(AI.with(|c| c.borrow().last.clone())),
        AiTokens => Value::Int(AI.with(|c| c.borrow().last_tokens)),
        AiJson => {
            let prompt = text(&args[0]).to_string();
            let answer = AI.with(|c| {
                let mut cfg = c.borrow_mut();
                let (ans, tok) = ai_send(&cfg, ai_messages(&cfg, Some(&prompt)), true)?;
                cfg.last = ans.clone();
                cfg.last_tokens = tok;
                Ok::<String, String>(ans)
            })?;
            string(answer)
        }
        AiEmbed => {
            let input = text(&args[0]).to_string();
            let v = AI.with(|c| ai_embed(&c.borrow(), &input))?;
            grid_val(v)
        }
        AiModels => {
            let list = AI.with(|c| ai_models(&c.borrow()))?;
            Value::Seq(Rc::new(RefCell::new(list.into_iter().map(string).collect())))
        }

        // ======== module "symbol": symbolic mathematics ========
        SymVar => Value::Int(SYM.with(|s| s_push(&mut s.borrow_mut(), SNode::Var(text(&args[0]).to_string())) as i64)),
        SymConst => Value::Int(SYM.with(|s| s_push(&mut s.borrow_mut(), SNode::Const(real(&args[0]))) as i64)),
        SymAdd | SymSub | SymMul | SymDiv | SymPow => {
            let op = match b {
                SymAdd => 0,
                SymSub => 1,
                SymMul => 2,
                SymDiv => 3,
                _ => 4,
            };
            Value::Int(SYM.with(|s| {
                let mut v = s.borrow_mut();
                let (a, c) = (s_check(v.len(), int(&args[0]))?, s_check(v.len(), int(&args[1]))?);
                Ok::<i64, String>(s_push(&mut v, SNode::Bin(op, a, c)) as i64)
            })?)
        }
        SymNeg | SymSin | SymCos | SymExp | SymLn => {
            let op = match b {
                SymNeg => 0,
                SymSin => 1,
                SymCos => 2,
                SymExp => 3,
                _ => 4,
            };
            Value::Int(SYM.with(|s| {
                let mut v = s.borrow_mut();
                let a = s_check(v.len(), int(&args[0]))?;
                Ok::<i64, String>(s_push(&mut v, SNode::Un(op, a)) as i64)
            })?)
        }
        SymDiff => Value::Int(SYM.with(|s| {
            let mut v = s.borrow_mut();
            let e = s_check(v.len(), int(&args[0]))?;
            let vh = s_check(v.len(), int(&args[1]))?;
            let var = match &v[vh] {
                SNode::Var(n) => n.clone(),
                _ => return Err("symbol.diff: the second argument must be a symbol.var".to_string()),
            };
            Ok::<i64, String>(s_diff(&mut v, e, &var) as i64)
        })?),
        SymSimplify => Value::Int(SYM.with(|s| {
            let mut v = s.borrow_mut();
            let e = s_check(v.len(), int(&args[0]))?;
            Ok::<i64, String>(s_simplify(&mut v, e) as i64)
        })?),
        SymEval => Value::Real(SYM.with(|s| {
            let v = s.borrow();
            let e = s_check(v.len(), int(&args[0]))?;
            let vh = s_check(v.len(), int(&args[1]))?;
            let var = match &v[vh] {
                SNode::Var(n) => n.clone(),
                _ => return Err("symbol.eval: the second argument must be a symbol.var".to_string()),
            };
            s_eval(&v, e, &var, real(&args[2]))
        })?),
        SymText => string(SYM.with(|s| {
            let v = s.borrow();
            let e = s_check(v.len(), int(&args[0]))?;
            Ok::<String, String>(s_text(&v, e))
        })?),
        SymReset => {
            SYM.with(|s| s.borrow_mut().clear());
            Value::Null
        }

        // ======== module "synth": audio synthesis / DSP ========
        SynthRate => {
            SRATE.with(|r| r.set(int(&args[0]).clamp(1, 384000) as u32));
            Value::Null
        }
        SynthBuffer | SynthSilence => {
            let n = (real(&args[0]) * syn_rate() as f64).max(0.0) as usize;
            Value::Int(syn_push(vec![0.0; n]))
        }
        SynthTone => {
            let (freq, secs) = (real(&args[0]), real(&args[1]));
            let wave = text(&args[2]).to_string();
            let rate = syn_rate() as f64;
            let n = (secs * rate).max(0.0) as usize;
            let mut buf = Vec::with_capacity(n);
            let mut seed: u32 = 0x1234_5678;
            for i in 0..n {
                let t = i as f64 / rate;
                let ph = (freq * t).fract(); // 0..1 within a cycle
                let s = match wave.as_str() {
                    "square" => {
                        if ph < 0.5 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    "saw" => 2.0 * ph - 1.0,
                    "triangle" => 1.0 - 4.0 * (ph - 0.5).abs(),
                    "noise" => {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        (seed >> 8) as f64 / 8_388_608.0 - 1.0
                    }
                    _ => (2.0 * std::f64::consts::PI * freq * t).sin(),
                };
                buf.push(s as f32);
            }
            Value::Int(syn_push(buf))
        }
        SynthGain => {
            let h = int(&args[0]);
            let g = real(&args[1]) as f32;
            syn_mut(h, |b| b.iter_mut().for_each(|s| *s *= g))?;
            Value::Int(h)
        }
        SynthMix => {
            let (a, b) = (int(&args[0]), int(&args[1]));
            let other = syn_get(b, Clone::clone)?;
            syn_mut(a, |dst| {
                if dst.len() < other.len() {
                    dst.resize(other.len(), 0.0);
                }
                for (d, s) in dst.iter_mut().zip(&other) {
                    *d += s;
                }
            })?;
            Value::Int(a)
        }
        SynthConcat => {
            let (a, b) = (int(&args[0]), int(&args[1]));
            let other = syn_get(b, Clone::clone)?;
            syn_mut(a, |dst| dst.extend_from_slice(&other))?;
            Value::Int(a)
        }
        SynthFade => {
            let h = int(&args[0]);
            let rate = syn_rate() as f64;
            let (at, rel) = ((real(&args[1]) * rate) as usize, (real(&args[2]) * rate) as usize);
            syn_mut(h, |b| {
                let n = b.len();
                for (i, s) in b.iter_mut().enumerate() {
                    let mut g = 1.0f32;
                    if at > 0 && i < at {
                        g = g.min(i as f32 / at as f32);
                    }
                    if rel > 0 && i >= n.saturating_sub(rel) {
                        g = g.min((n - i) as f32 / rel as f32);
                    }
                    *s *= g;
                }
            })?;
            Value::Int(h)
        }
        SynthLowpass => {
            let h = int(&args[0]);
            let cutoff = real(&args[1]).max(1.0);
            let rate = syn_rate() as f64;
            let dt = 1.0 / rate;
            let rc = 1.0 / (2.0 * std::f64::consts::PI * cutoff);
            let alpha = (dt / (rc + dt)) as f32;
            syn_mut(h, |b| {
                let mut prev = 0.0f32;
                for s in b.iter_mut() {
                    prev += alpha * (*s - prev);
                    *s = prev;
                }
            })?;
            Value::Int(h)
        }
        SynthHighpass => {
            let h = int(&args[0]);
            let cutoff = real(&args[1]).max(1.0);
            let rate = syn_rate() as f64;
            let dt = 1.0 / rate;
            let rc = 1.0 / (2.0 * std::f64::consts::PI * cutoff);
            let alpha = (rc / (rc + dt)) as f32;
            syn_mut(h, |b| {
                let mut prev_in = 0.0f32;
                let mut prev_out = 0.0f32;
                for s in b.iter_mut() {
                    let out = alpha * (prev_out + *s - prev_in);
                    prev_in = *s;
                    prev_out = out;
                    *s = out;
                }
            })?;
            Value::Int(h)
        }
        SynthNormalize => {
            let h = int(&args[0]);
            let peak = syn_get(h, |b| b.iter().fold(0.0f32, |m, s| m.max(s.abs())))?;
            if peak > 0.0 {
                syn_mut(h, |b| b.iter_mut().for_each(|s| *s /= peak))?;
            }
            Value::Int(h)
        }
        SynthLength => Value::Int(syn_get(int(&args[0]), |b| b.len() as i64)?),
        SynthDuration => Value::Real(syn_get(int(&args[0]), |b| b.len() as f64 / syn_rate() as f64)?),
        SynthPeak => Value::Real(syn_get(int(&args[0]), |b| b.iter().fold(0.0f32, |m, s| m.max(s.abs())) as f64)?),
        SynthRms => Value::Real(syn_get(int(&args[0]), |b| {
            if b.is_empty() {
                0.0
            } else {
                (b.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / b.len() as f64).sqrt()
            }
        })?),
        SynthDominant => {
            let rate = syn_rate() as f64;
            Value::Real(syn_get(int(&args[0]), |b| {
                let n = b.len();
                if n < 2 {
                    return 0.0;
                }
                let (mut best, mut bestmag) = (0.0, -1.0);
                for k in 1..n / 2 {
                    let w = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
                    let (mut re, mut im) = (0.0, 0.0);
                    for (i, &s) in b.iter().enumerate() {
                        let a = w * i as f64;
                        re += s as f64 * a.cos();
                        im += s as f64 * a.sin();
                    }
                    let mag = re * re + im * im;
                    if mag > bestmag {
                        bestmag = mag;
                        best = k as f64 * rate / n as f64;
                    }
                }
                best
            })?)
        }
        SynthSaveWav => {
            let path = text(&args[1]).to_string();
            let bytes = syn_get(int(&args[0]), |b| syn_wav_bytes(b, syn_rate()))?;
            std::fs::write(&path, bytes).map_err(|e| format!("synth.save_wav: cannot write {}: {}", path, e))?;
            Value::Null
        }
        SynthLoadWav => {
            let (samples, rate) = syn_read_wav(text(&args[0]))?;
            SRATE.with(|r| r.set(rate));
            Value::Int(syn_push(samples))
        }
        SynthPlay => {
            let path = text(&args[0]).to_string();
            #[cfg(windows)]
            let r = std::process::Command::new("cmd").args(["/C", "start", "", &path]).spawn();
            #[cfg(target_os = "macos")]
            let r = std::process::Command::new("open").arg(&path).spawn();
            #[cfg(not(any(windows, target_os = "macos")))]
            let r = std::process::Command::new("xdg-open").arg(&path).spawn();
            r.map_err(|e| format!("synth.play: cannot play {}: {}", path, e))?;
            Value::Null
        }
        SynthFree => {
            SYNTH.with(|r| {
                if let Some(slot) = usize::try_from(int(&args[0])).ok().and_then(|i| r.borrow_mut().get_mut(i).map(|s| *s = None)) {
                    let _ = slot;
                }
            });
            Value::Null
        }

        // ======== module "gwsim": 2D physics ========
        SimReset => {
            SIM.with(|s| *s.borrow_mut() = SimWorld::default());
            Value::Null
        }
        SimBody => {
            let (x, y, r, m) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
            Value::Int(SIM.with(|s| {
                let mut w = s.borrow_mut();
                w.bodies.push(Body { x, y, vx: 0.0, vy: 0.0, r, m: m.max(1e-9), fixed: false });
                (w.bodies.len() - 1) as i64
            }))
        }
        SimFix => {
            let i = sim_body(int(&args[0]))?;
            let fixed = matches!(&args[1], Value::Bool(true));
            SIM.with(|s| s.borrow_mut().bodies[i].fixed = fixed);
            Value::Null
        }
        SimGravity => {
            let (gx, gy) = (real(&args[0]), real(&args[1]));
            SIM.with(|s| {
                let mut w = s.borrow_mut();
                w.gx = gx;
                w.gy = gy;
            });
            Value::Null
        }
        SimBounds => {
            let (x, y, w, h, rest) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
            SIM.with(|s| {
                let world = &mut *s.borrow_mut();
                world.bx0 = x;
                world.by0 = y;
                world.bx1 = x + w;
                world.by1 = y + h;
                world.rest = rest;
            });
            Value::Null
        }
        SimPush => {
            let i = sim_body(int(&args[0]))?;
            let (fx, fy) = (real(&args[1]), real(&args[2]));
            SIM.with(|s| {
                let mut w = s.borrow_mut();
                w.bodies[i].vx += fx;
                w.bodies[i].vy += fy;
            });
            Value::Null
        }
        SimSetVel => {
            let i = sim_body(int(&args[0]))?;
            let (vx, vy) = (real(&args[1]), real(&args[2]));
            SIM.with(|s| {
                let mut w = s.borrow_mut();
                w.bodies[i].vx = vx;
                w.bodies[i].vy = vy;
            });
            Value::Null
        }
        SimSetPos => {
            let i = sim_body(int(&args[0]))?;
            let (x, y) = (real(&args[1]), real(&args[2]));
            SIM.with(|s| {
                let mut w = s.borrow_mut();
                w.bodies[i].x = x;
                w.bodies[i].y = y;
            });
            Value::Null
        }
        SimStep => {
            sim_step(real(&args[0]));
            Value::Null
        }
        SimX => {
            let i = sim_body(int(&args[0]))?;
            Value::Real(SIM.with(|s| s.borrow().bodies[i].x))
        }
        SimY => {
            let i = sim_body(int(&args[0]))?;
            Value::Real(SIM.with(|s| s.borrow().bodies[i].y))
        }
        SimVx => {
            let i = sim_body(int(&args[0]))?;
            Value::Real(SIM.with(|s| s.borrow().bodies[i].vx))
        }
        SimVy => {
            let i = sim_body(int(&args[0]))?;
            Value::Real(SIM.with(|s| s.borrow().bodies[i].vy))
        }
        SimCount => Value::Int(SIM.with(|s| s.borrow().bodies.len() as i64)),
        SimCollide => {
            let (i, j) = (sim_body(int(&args[0]))?, sim_body(int(&args[1]))?);
            Value::Bool(SIM.with(|s| {
                let w = s.borrow();
                let (a, c) = (w.bodies[i], w.bodies[j]);
                (a.x - c.x).hypot(a.y - c.y) < a.r + c.r
            }))
        }

        // ======== module "gwframe": numeric dataframe ========
        FrameNew => Value::Int(frame_push(Frame { names: Vec::new(), cols: Vec::new() })),
        FrameFromCsv => Value::Int(frame_push(frame_from_csv(text(&args[0]))?)),
        FrameRows => Value::Int(with_frame(int(&args[0]), |f| Ok(frame_rows(f) as i64))?),
        FrameCols => Value::Int(with_frame(int(&args[0]), |f| Ok(f.names.len() as i64))?),
        FrameColname => with_frame(int(&args[0]), |f| {
            let i = int(&args[1]);
            usize::try_from(i - 1)
                .ok()
                .and_then(|i| f.names.get(i))
                .cloned()
                .ok_or_else(|| format!("gwframe.colname: no column {}", i))
        })
        .map(string)?,
        FrameGet => with_frame(int(&args[0]), |f| {
            let row = usize::try_from(int(&args[1]) - 1).map_err(|_| "gwframe.get: row out of range".to_string())?;
            let col = frame_col(f, text(&args[2]))?;
            col.get(row).copied().ok_or_else(|| "gwframe.get: row out of range".to_string())
        })
        .map(Value::Real)?,
        FrameSum => Value::Real(with_frame(int(&args[0]), |f| Ok(frame_col(f, text(&args[1]))?.iter().sum()))?),
        FrameMean => Value::Real(with_frame(int(&args[0]), |f| {
            let c = frame_col(f, text(&args[1]))?;
            Ok(if c.is_empty() { 0.0 } else { c.iter().sum::<f64>() / c.len() as f64 })
        })?),
        FrameMin => Value::Real(with_frame(int(&args[0]), |f| {
            Ok(frame_col(f, text(&args[1]))?.iter().copied().fold(f64::INFINITY, f64::min))
        })?),
        FrameMax => Value::Real(with_frame(int(&args[0]), |f| {
            Ok(frame_col(f, text(&args[1]))?.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        })?),
        FrameStd => Value::Real(with_frame(int(&args[0]), |f| {
            let c = frame_col(f, text(&args[1]))?;
            if c.is_empty() {
                return Ok(0.0);
            }
            let m = c.iter().sum::<f64>() / c.len() as f64;
            Ok((c.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / c.len() as f64).sqrt())
        })?),
        FrameFilter => {
            let (name, op, val) = (text(&args[1]), text(&args[2]).to_string(), real(&args[3]));
            let newf = with_frame(int(&args[0]), |f| {
                let c = frame_col(f, name)?;
                let keep: Vec<bool> = c
                    .iter()
                    .map(|&x| match op.as_str() {
                        ">" => x > val,
                        "<" => x < val,
                        ">=" => x >= val,
                        "<=" => x <= val,
                        "==" => x == val,
                        "!=" => x != val,
                        _ => false,
                    })
                    .collect();
                if !matches!(op.as_str(), ">" | "<" | ">=" | "<=" | "==" | "!=") {
                    return Err(format!("gwframe.filter: unknown operator \"{}\" (use > < >= <= == !=)", op));
                }
                Ok(frame_select(f, &keep))
            })?;
            Value::Int(frame_push(newf))
        }
        FrameSort => {
            let (name, asc) = (text(&args[1]), matches!(&args[2], Value::Bool(true)));
            let newf = with_frame(int(&args[0]), |f| {
                let c = frame_col(f, name)?;
                let mut order: Vec<usize> = (0..c.len()).collect();
                order.sort_by(|&i, &j| {
                    let o = c[i].partial_cmp(&c[j]).unwrap_or(Ordering::Equal);
                    if asc { o } else { o.reverse() }
                });
                Ok(frame_reorder(f, &order))
            })?;
            Value::Int(frame_push(newf))
        }
        FrameHead => {
            let n = int(&args[1]).max(0) as usize;
            let newf = with_frame(int(&args[0]), |f| {
                let keep: Vec<bool> = (0..frame_rows(f)).map(|i| i < n).collect();
                Ok(frame_select(f, &keep))
            })?;
            Value::Int(frame_push(newf))
        }
        FrameGroupSum | FrameGroupMean => {
            let mean = matches!(b, FrameGroupMean);
            let (kname, vname) = (text(&args[1]), text(&args[2]));
            let newf = with_frame(int(&args[0]), |f| {
                let (kc, vc) = (frame_col(f, kname)?, frame_col(f, vname)?);
                let mut keys: Vec<f64> = Vec::new();
                let mut sums: Vec<f64> = Vec::new();
                let mut counts: Vec<f64> = Vec::new();
                for (k, v) in kc.iter().zip(vc) {
                    match keys.iter().position(|x| x == k) {
                        Some(i) => {
                            sums[i] += v;
                            counts[i] += 1.0;
                        }
                        None => {
                            keys.push(*k);
                            sums.push(*v);
                            counts.push(1.0);
                        }
                    }
                }
                if mean {
                    for i in 0..sums.len() {
                        sums[i] /= counts[i];
                    }
                }
                Ok(Frame { names: vec![kname.to_string(), vname.to_string()], cols: vec![keys, sums] })
            })?;
            Value::Int(frame_push(newf))
        }
        FrameToCsv => {
            let path = text(&args[1]).to_string();
            let out = with_frame(int(&args[0]), |f| {
                let mut s = f.names.join(",");
                s.push('\n');
                for r in 0..frame_rows(f) {
                    let row: Vec<String> = f.cols.iter().map(|c| format_real(c[r])).collect();
                    s.push_str(&row.join(","));
                    s.push('\n');
                }
                Ok(s)
            })?;
            std::fs::write(&path, out).map_err(|e| format!("gwframe.to_csv: cannot write {}: {}", path, e))?;
            Value::Null
        }
        FrameShow => {
            let n = int(&args[1]).max(0) as usize;
            let out = with_frame(int(&args[0]), |f| {
                let mut s = f.names.join(" | ");
                s.push('\n');
                for r in 0..frame_rows(f).min(n) {
                    let row: Vec<String> = f.cols.iter().map(|c| format_real(c[r])).collect();
                    s.push_str(&row.join(" | "));
                    s.push('\n');
                }
                s.push_str(&format!("[{} rows x {} cols]", frame_rows(f), f.names.len()));
                Ok(s)
            })?;
            string(out)
        }
        FrameFree => {
            FRAMES.with(|r| {
                if let Some(slot) = usize::try_from(int(&args[0])).ok().and_then(|i| r.borrow_mut().get_mut(i).map(|s| *s = None)) {
                    slot;
                }
            });
            Value::Null
        }

        // ======== batch M: 615 modules ========
        Mk0001 => Value::Real(real(&args[0]) * 1e-10),
        Mk0002 => Value::Real(real(&args[0]) / 1e-10),
        Mk0003 => Value::Real(real(&args[0]) * 1e-09),
        Mk0004 => Value::Real(real(&args[0]) / 1e-09),
        Mk0005 => Value::Real(real(&args[0]) * 1e-06),
        Mk0006 => Value::Real(real(&args[0]) / 1e-06),
        Mk0007 => Value::Real(real(&args[0]) * 0.001),
        Mk0008 => Value::Real(real(&args[0]) / 0.001),
        Mk0009 => Value::Real(real(&args[0]) * 0.01),
        Mk0010 => Value::Real(real(&args[0]) / 0.01),
        Mk0011 => Value::Real(real(&args[0]) * 0.1),
        Mk0012 => Value::Real(real(&args[0]) / 0.1),
        Mk0013 => Value::Real(real(&args[0]) * 10.0),
        Mk0014 => Value::Real(real(&args[0]) / 10.0),
        Mk0015 => Value::Real(real(&args[0]) * 100.0),
        Mk0016 => Value::Real(real(&args[0]) / 100.0),
        Mk0017 => Value::Real(real(&args[0]) * 1000.0),
        Mk0018 => Value::Real(real(&args[0]) / 1000.0),
        Mk0019 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0020 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0021 => Value::Real(real(&args[0]) * 0.0254),
        Mk0022 => Value::Real(real(&args[0]) / 0.0254),
        Mk0023 => Value::Real(real(&args[0]) * 0.3048),
        Mk0024 => Value::Real(real(&args[0]) / 0.3048),
        Mk0025 => Value::Real(real(&args[0]) * 0.9144),
        Mk0026 => Value::Real(real(&args[0]) / 0.9144),
        Mk0027 => Value::Real(real(&args[0]) * 1609.344),
        Mk0028 => Value::Real(real(&args[0]) / 1609.344),
        Mk0029 => Value::Real(real(&args[0]) * 201.168),
        Mk0030 => Value::Real(real(&args[0]) / 201.168),
        Mk0031 => Value::Real(real(&args[0]) * 20.1168),
        Mk0032 => Value::Real(real(&args[0]) / 20.1168),
        Mk0033 => Value::Real(real(&args[0]) * 5.0292),
        Mk0034 => Value::Real(real(&args[0]) / 5.0292),
        Mk0035 => Value::Real(real(&args[0]) * 5.0292),
        Mk0036 => Value::Real(real(&args[0]) / 5.0292),
        Mk0037 => Value::Real(real(&args[0]) * 5.0292),
        Mk0038 => Value::Real(real(&args[0]) / 5.0292),
        Mk0039 => Value::Real(real(&args[0]) * 0.201168),
        Mk0040 => Value::Real(real(&args[0]) / 0.201168),
        Mk0041 => Value::Real(real(&args[0]) * 1.8288),
        Mk0042 => Value::Real(real(&args[0]) / 1.8288),
        Mk0043 => Value::Real(real(&args[0]) * 185.2),
        Mk0044 => Value::Real(real(&args[0]) / 185.2),
        Mk0045 => Value::Real(real(&args[0]) * 1852.0),
        Mk0046 => Value::Real(real(&args[0]) / 1852.0),
        Mk0047 => Value::Real(real(&args[0]) * 4828.032),
        Mk0048 => Value::Real(real(&args[0]) / 4828.032),
        Mk0049 => Value::Real(real(&args[0]) * 0.1016),
        Mk0050 => Value::Real(real(&args[0]) / 0.1016),
        Mk0051 => Value::Real(real(&args[0]) * 0.2286),
        Mk0052 => Value::Real(real(&args[0]) / 0.2286),
        Mk0053 => Value::Real(real(&args[0]) * 0.4572),
        Mk0054 => Value::Real(real(&args[0]) / 0.4572),
        Mk0055 => Value::Real(real(&args[0]) * 1.143),
        Mk0056 => Value::Real(real(&args[0]) / 1.143),
        Mk0057 => Value::Real(real(&args[0]) * 0.0084667),
        Mk0058 => Value::Real(real(&args[0]) / 0.0084667),
        Mk0059 => Value::Real(real(&args[0]) * 2.54e-05),
        Mk0060 => Value::Real(real(&args[0]) / 2.54e-05),
        Mk0061 => Value::Real(real(&args[0]) * 2.54e-05),
        Mk0062 => Value::Real(real(&args[0]) / 2.54e-05),
        Mk0063 => Value::Real(real(&args[0]) * 0.00423333),
        Mk0064 => Value::Real(real(&args[0]) / 0.00423333),
        Mk0065 => Value::Real(real(&args[0]) * 0.000352778),
        Mk0066 => Value::Real(real(&args[0]) / 0.000352778),
        Mk0067 => Value::Real(real(&args[0]) * 1.76389e-05),
        Mk0068 => Value::Real(real(&args[0]) / 1.76389e-05),
        Mk0069 => Value::Real(real(&args[0]) * 9460730472580800.0),
        Mk0070 => Value::Real(real(&args[0]) / 9460730472580800.0),
        Mk0071 => Value::Real(real(&args[0]) * 3.0856775814914e+16),
        Mk0072 => Value::Real(real(&args[0]) / 3.0856775814914e+16),
        Mk0073 => Value::Real(real(&args[0]) * 149597870700.0),
        Mk0074 => Value::Real(real(&args[0]) / 149597870700.0),
        Mk0075 => Value::Real(real(&args[0]) * 299792458.0),
        Mk0076 => Value::Real(real(&args[0]) / 299792458.0),
        Mk0077 => Value::Real(real(&args[0]) * 17987547480.0),
        Mk0078 => Value::Real(real(&args[0]) / 17987547480.0),
        Mk0079 => Value::Real(real(&args[0]) * 384400000.0),
        Mk0080 => Value::Real(real(&args[0]) / 384400000.0),
        Mk0081 => Value::Real(real(&args[0]) * 695700000.0),
        Mk0082 => Value::Real(real(&args[0]) / 695700000.0),
        Mk0083 => Value::Real(real(&args[0]) * 6371000.0),
        Mk0084 => Value::Real(real(&args[0]) / 6371000.0),
        Mk0085 => Value::Real(real(&args[0]) * 1.616255e-35),
        Mk0086 => Value::Real(real(&args[0]) / 1.616255e-35),
        Mk0087 => Value::Real(real(&args[0]) * 5.29177210903e-11),
        Mk0088 => Value::Real(real(&args[0]) / 5.29177210903e-11),
        Mk0089 => Value::Real(real(&args[0]) * 1e-15),
        Mk0090 => Value::Real(real(&args[0]) / 1e-15),
        Mk0091 => Value::Real(real(&args[0]) * 1.0021e-13),
        Mk0092 => Value::Real(real(&args[0]) / 1.0021e-13),
        Mk0093 => Value::Real(real(&args[0]) * 1.7018),
        Mk0094 => Value::Real(real(&args[0]) / 1.7018),
        Mk0095 => Value::Real(real(&args[0]) * 6.096),
        Mk0096 => Value::Real(real(&args[0]) / 6.096),
        Mk0097 => Value::Real(real(&args[0]) * 27.432),
        Mk0098 => Value::Real(real(&args[0]) / 27.432),
        Mk0099 => Value::Real(real(&args[0]) * 0.022225),
        Mk0100 => Value::Real(real(&args[0]) / 0.022225),
        Mk0101 => Value::Real(real(&args[0]) * 0.05715),
        Mk0102 => Value::Real(real(&args[0]) / 0.05715),
        Mk0103 => Value::Real(real(&args[0]) * 0.0762),
        Mk0104 => Value::Real(real(&args[0]) / 0.0762),
        Mk0105 => Value::Real(real(&args[0]) * 0.019),
        Mk0106 => Value::Real(real(&args[0]) / 0.019),
        Mk0107 => Value::Real(real(&args[0]) * 185.0),
        Mk0108 => Value::Real(real(&args[0]) / 185.0),
        Mk0109 => Value::Real(real(&args[0]) * 3.7783),
        Mk0110 => Value::Real(real(&args[0]) / 3.7783),
        Mk0111 => Value::Real(real(&args[0]) * 1000.0),
        Mk0112 => Value::Real(real(&args[0]) / 1000.0),
        Mk0113 => Value::Real(real(&args[0]) * 10000.0),
        Mk0114 => Value::Real(real(&args[0]) / 10000.0),
        Mk0115 => Value::Real(real(&args[0]) * 1e-18),
        Mk0116 => Value::Real(real(&args[0]) / 1e-18),
        Mk0117 => Value::Real(real(&args[0]) * 1e-15),
        Mk0118 => Value::Real(real(&args[0]) / 1e-15),
        Mk0119 => Value::Real(real(&args[0]) * 1e-12),
        Mk0120 => Value::Real(real(&args[0]) / 1e-12),
        Mk0121 => Value::Real(real(&args[0]) * 1000000000.0),
        Mk0122 => Value::Real(real(&args[0]) / 1000000000.0),
        Mk0123 => Value::Real(real(&args[0]) * 1000000000000.0),
        Mk0124 => Value::Real(real(&args[0]) / 1000000000000.0),
        Mk0125 => Value::Real(real(&args[0]) * 0.000254),
        Mk0126 => Value::Real(real(&args[0]) / 0.000254),
        Mk0127 => Value::Real(real(&args[0]) * 0.000352778),
        Mk0128 => Value::Real(real(&args[0]) / 0.000352778),
        Mk0129 => Value::Real(real(&args[0]) * 1e-06),
        Mk0130 => Value::Real(real(&args[0]) / 1e-06),
        Mk0131 => Value::Real(real(&args[0]) * 1e-05),
        Mk0132 => Value::Real(real(&args[0]) / 1e-05),
        Mk0133 => Value::Real(real(&args[0]) * 0.0001),
        Mk0134 => Value::Real(real(&args[0]) / 0.0001),
        Mk0135 => Value::Real(real(&args[0]) * 0.001),
        Mk0136 => Value::Real(real(&args[0]) / 0.001),
        Mk0137 => Value::Real(real(&args[0]) * 0.01),
        Mk0138 => Value::Real(real(&args[0]) / 0.01),
        Mk0139 => Value::Real(real(&args[0]) * 0.1),
        Mk0140 => Value::Real(real(&args[0]) / 0.1),
        Mk0141 => Value::Real(real(&args[0]) * 1000.0),
        Mk0142 => Value::Real(real(&args[0]) / 1000.0),
        Mk0143 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0144 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0145 => Value::Real(real(&args[0]) * 1000000000.0),
        Mk0146 => Value::Real(real(&args[0]) / 1000000000.0),
        Mk0147 => Value::Real(real(&args[0]) * 1e-09),
        Mk0148 => Value::Real(real(&args[0]) / 1e-09),
        Mk0149 => Value::Real(real(&args[0]) * 0.028349523125),
        Mk0150 => Value::Real(real(&args[0]) / 0.028349523125),
        Mk0151 => Value::Real(real(&args[0]) * 0.45359237),
        Mk0152 => Value::Real(real(&args[0]) / 0.45359237),
        Mk0153 => Value::Real(real(&args[0]) * 12.70058636),
        Mk0154 => Value::Real(real(&args[0]) / 12.70058636),
        Mk0155 => Value::Real(real(&args[0]) * 50.80234544),
        Mk0156 => Value::Real(real(&args[0]) / 50.80234544),
        Mk0157 => Value::Real(real(&args[0]) * 1016.0469088),
        Mk0158 => Value::Real(real(&args[0]) / 1016.0469088),
        Mk0159 => Value::Real(real(&args[0]) * 907.18474),
        Mk0160 => Value::Real(real(&args[0]) / 907.18474),
        Mk0161 => Value::Real(real(&args[0]) * 6.479891e-05),
        Mk0162 => Value::Real(real(&args[0]) / 6.479891e-05),
        Mk0163 => Value::Real(real(&args[0]) * 0.0017718451953125),
        Mk0164 => Value::Real(real(&args[0]) / 0.0017718451953125),
        Mk0165 => Value::Real(real(&args[0]) * 0.0311034768),
        Mk0166 => Value::Real(real(&args[0]) / 0.0311034768),
        Mk0167 => Value::Real(real(&args[0]) * 0.3732417216),
        Mk0168 => Value::Real(real(&args[0]) / 0.3732417216),
        Mk0169 => Value::Real(real(&args[0]) * 0.00155517384),
        Mk0170 => Value::Real(real(&args[0]) / 0.00155517384),
        Mk0171 => Value::Real(real(&args[0]) * 0.0002),
        Mk0172 => Value::Real(real(&args[0]) / 0.0002),
        Mk0173 => Value::Real(real(&args[0]) * 1.6605390666e-27),
        Mk0174 => Value::Real(real(&args[0]) / 1.6605390666e-27),
        Mk0175 => Value::Real(real(&args[0]) * 9.1093837015e-31),
        Mk0176 => Value::Real(real(&args[0]) / 9.1093837015e-31),
        Mk0177 => Value::Real(real(&args[0]) * 1.67262192369e-27),
        Mk0178 => Value::Real(real(&args[0]) / 1.67262192369e-27),
        Mk0179 => Value::Real(real(&args[0]) * 2.176434e-08),
        Mk0180 => Value::Real(real(&args[0]) / 2.176434e-08),
        Mk0181 => Value::Real(real(&args[0]) * 1.98892e+30),
        Mk0182 => Value::Real(real(&args[0]) / 1.98892e+30),
        Mk0183 => Value::Real(real(&args[0]) * 5.9722e+24),
        Mk0184 => Value::Real(real(&args[0]) / 5.9722e+24),
        Mk0185 => Value::Real(real(&args[0]) * 1.898e+27),
        Mk0186 => Value::Real(real(&args[0]) / 1.898e+27),
        Mk0187 => Value::Real(real(&args[0]) * 40.8233),
        Mk0188 => Value::Real(real(&args[0]) / 40.8233),
        Mk0189 => Value::Real(real(&args[0]) * 100.0),
        Mk0190 => Value::Real(real(&args[0]) / 100.0),
        Mk0191 => Value::Real(real(&args[0]) * 453.59237),
        Mk0192 => Value::Real(real(&args[0]) / 453.59237),
        Mk0193 => Value::Real(real(&args[0]) * 0.0012959782),
        Mk0194 => Value::Real(real(&args[0]) / 0.0012959782),
        Mk0195 => Value::Real(real(&args[0]) * 0.00057),
        Mk0196 => Value::Real(real(&args[0]) / 0.00057),
        Mk0197 => Value::Real(real(&args[0]) * 0.6),
        Mk0198 => Value::Real(real(&args[0]) / 0.6),
        Mk0199 => Value::Real(real(&args[0]) * 26.0),
        Mk0200 => Value::Real(real(&args[0]) / 26.0),
        Mk0201 => Value::Real(real(&args[0]) * 0.0114),
        Mk0202 => Value::Real(real(&args[0]) / 0.0114),
        Mk0203 => Value::Real(real(&args[0]) * 21540.0),
        Mk0204 => Value::Real(real(&args[0]) / 21540.0),
        Mk0205 => Value::Real(real(&args[0]) * 114.3),
        Mk0206 => Value::Real(real(&args[0]) / 114.3),
        Mk0207 => Value::Real(real(&args[0]) * 12.70058636),
        Mk0208 => Value::Real(real(&args[0]) / 12.70058636),
        Mk0209 => Value::Real(real(&args[0]) * 3.17514659),
        Mk0210 => Value::Real(real(&args[0]) / 3.17514659),
        Mk0211 => Value::Real(real(&args[0]) * 3.2399455e-06),
        Mk0212 => Value::Real(real(&args[0]) / 3.2399455e-06),
        Mk0213 => Value::Real(real(&args[0]) * 217.72),
        Mk0214 => Value::Real(real(&args[0]) / 217.72),
        Mk0215 => Value::Real(real(&args[0]) * 0.001),
        Mk0216 => Value::Real(real(&args[0]) / 0.001),
        Mk0217 => Value::Real(real(&args[0]) * 0.01),
        Mk0218 => Value::Real(real(&args[0]) / 0.01),
        Mk0219 => Value::Real(real(&args[0]) * 0.1),
        Mk0220 => Value::Real(real(&args[0]) / 0.1),
        Mk0221 => Value::Real(real(&args[0]) * 10.0),
        Mk0222 => Value::Real(real(&args[0]) / 10.0),
        Mk0223 => Value::Real(real(&args[0]) * 100.0),
        Mk0224 => Value::Real(real(&args[0]) / 100.0),
        Mk0225 => Value::Real(real(&args[0]) * 1000.0),
        Mk0226 => Value::Real(real(&args[0]) / 1000.0),
        Mk0227 => Value::Real(real(&args[0]) * 1e-06),
        Mk0228 => Value::Real(real(&args[0]) / 1e-06),
        Mk0229 => Value::Real(real(&args[0]) * 1000.0),
        Mk0230 => Value::Real(real(&args[0]) / 1000.0),
        Mk0231 => Value::Real(real(&args[0]) * 0.001),
        Mk0232 => Value::Real(real(&args[0]) / 0.001),
        Mk0233 => Value::Real(real(&args[0]) * 0.016387064),
        Mk0234 => Value::Real(real(&args[0]) / 0.016387064),
        Mk0235 => Value::Real(real(&args[0]) * 28.316846592),
        Mk0236 => Value::Real(real(&args[0]) / 28.316846592),
        Mk0237 => Value::Real(real(&args[0]) * 764.554857984),
        Mk0238 => Value::Real(real(&args[0]) / 764.554857984),
        Mk0239 => Value::Real(real(&args[0]) * 3.785411784),
        Mk0240 => Value::Real(real(&args[0]) / 3.785411784),
        Mk0241 => Value::Real(real(&args[0]) * 4.54609),
        Mk0242 => Value::Real(real(&args[0]) / 4.54609),
        Mk0243 => Value::Real(real(&args[0]) * 0.946352946),
        Mk0244 => Value::Real(real(&args[0]) / 0.946352946),
        Mk0245 => Value::Real(real(&args[0]) * 1.1365225),
        Mk0246 => Value::Real(real(&args[0]) / 1.1365225),
        Mk0247 => Value::Real(real(&args[0]) * 0.473176473),
        Mk0248 => Value::Real(real(&args[0]) / 0.473176473),
        Mk0249 => Value::Real(real(&args[0]) * 0.56826125),
        Mk0250 => Value::Real(real(&args[0]) / 0.56826125),
        Mk0251 => Value::Real(real(&args[0]) * 0.2365882365),
        Mk0252 => Value::Real(real(&args[0]) / 0.2365882365),
        Mk0253 => Value::Real(real(&args[0]) * 0.25),
        Mk0254 => Value::Real(real(&args[0]) / 0.25),
        Mk0255 => Value::Real(real(&args[0]) * 0.0295735295625),
        Mk0256 => Value::Real(real(&args[0]) / 0.0295735295625),
        Mk0257 => Value::Real(real(&args[0]) * 0.0284130625),
        Mk0258 => Value::Real(real(&args[0]) / 0.0284130625),
        Mk0259 => Value::Real(real(&args[0]) * 0.01478676478125),
        Mk0260 => Value::Real(real(&args[0]) / 0.01478676478125),
        Mk0261 => Value::Real(real(&args[0]) * 0.00492892159375),
        Mk0262 => Value::Real(real(&args[0]) / 0.00492892159375),
        Mk0263 => Value::Real(real(&args[0]) * 0.11829411825),
        Mk0264 => Value::Real(real(&args[0]) / 0.11829411825),
        Mk0265 => Value::Real(real(&args[0]) * 0.1420653125),
        Mk0266 => Value::Real(real(&args[0]) / 0.1420653125),
        Mk0267 => Value::Real(real(&args[0]) * 6.1611519921875e-05),
        Mk0268 => Value::Real(real(&args[0]) / 6.1611519921875e-05),
        Mk0269 => Value::Real(real(&args[0]) * 0.003696691195312),
        Mk0270 => Value::Real(real(&args[0]) / 0.003696691195312),
        Mk0271 => Value::Real(real(&args[0]) * 158.987294928),
        Mk0272 => Value::Real(real(&args[0]) / 158.987294928),
        Mk0273 => Value::Real(real(&args[0]) * 119.240471196),
        Mk0274 => Value::Real(real(&args[0]) / 119.240471196),
        Mk0275 => Value::Real(real(&args[0]) * 163.65924),
        Mk0276 => Value::Real(real(&args[0]) / 163.65924),
        Mk0277 => Value::Real(real(&args[0]) * 238.480942392),
        Mk0278 => Value::Real(real(&args[0]) / 238.480942392),
        Mk0279 => Value::Real(real(&args[0]) * 40.91481),
        Mk0280 => Value::Real(real(&args[0]) / 40.91481),
        Mk0281 => Value::Real(real(&args[0]) * 81.82962),
        Mk0282 => Value::Real(real(&args[0]) / 81.82962),
        Mk0283 => Value::Real(real(&args[0]) * 8.809767541726),
        Mk0284 => Value::Real(real(&args[0]) / 8.809767541726),
        Mk0285 => Value::Real(real(&args[0]) * 35.23907016688),
        Mk0286 => Value::Real(real(&args[0]) / 35.23907016688),
        Mk0287 => Value::Real(real(&args[0]) * 36.36872),
        Mk0288 => Value::Real(real(&args[0]) / 36.36872),
        Mk0289 => Value::Real(real(&args[0]) * 9.09218),
        Mk0290 => Value::Real(real(&args[0]) / 9.09218),
        Mk0291 => Value::Real(real(&args[0]) * 3624.556363),
        Mk0292 => Value::Real(real(&args[0]) / 3624.556363),
        Mk0293 => Value::Real(real(&args[0]) * 2.359737216),
        Mk0294 => Value::Real(real(&args[0]) / 2.359737216),
        Mk0295 => Value::Real(real(&args[0]) * 1233481.837548),
        Mk0296 => Value::Real(real(&args[0]) / 1233481.837548),
        Mk0297 => Value::Real(real(&args[0]) * 0.0443603),
        Mk0298 => Value::Real(real(&args[0]) / 0.0443603),
        Mk0299 => Value::Real(real(&args[0]) * 0.0443603),
        Mk0300 => Value::Real(real(&args[0]) / 0.0443603),
        Mk0301 => Value::Real(real(&args[0]) * 0.0295735),
        Mk0302 => Value::Real(real(&args[0]) / 0.0295735),
        Mk0303 => Value::Real(real(&args[0]) * 1.5),
        Mk0304 => Value::Real(real(&args[0]) / 1.5),
        Mk0305 => Value::Real(real(&args[0]) * 3.0),
        Mk0306 => Value::Real(real(&args[0]) / 3.0),
        Mk0307 => Value::Real(real(&args[0]) * 6.0),
        Mk0308 => Value::Real(real(&args[0]) / 6.0),
        Mk0309 => Value::Real(real(&args[0]) * 15.0),
        Mk0310 => Value::Real(real(&args[0]) / 15.0),
        Mk0311 => Value::Real(real(&args[0]) * 953.923769),
        Mk0312 => Value::Real(real(&args[0]) / 953.923769),
        Mk0313 => Value::Real(real(&args[0]) * 476.961884),
        Mk0314 => Value::Real(real(&args[0]) / 476.961884),
        Mk0315 => Value::Real(real(&args[0]) * 318.0),
        Mk0316 => Value::Real(real(&args[0]) / 318.0),
        Mk0317 => Value::Real(real(&args[0]) * 1000.0),
        Mk0318 => Value::Real(real(&args[0]) / 1000.0),
        Mk0319 => Value::Real(real(&args[0]) * 1e-06),
        Mk0320 => Value::Real(real(&args[0]) / 1e-06),
        Mk0321 => Value::Real(real(&args[0]) * 5e-05),
        Mk0322 => Value::Real(real(&args[0]) / 5e-05),
        Mk0323 => Value::Real(real(&args[0]) * 18.18436),
        Mk0324 => Value::Real(real(&args[0]) / 18.18436),
        Mk0325 => Value::Real(real(&args[0]) * 2.273045),
        Mk0326 => Value::Real(real(&args[0]) / 2.273045),
        Mk0327 => Value::Real(real(&args[0]) * 0.142065),
        Mk0328 => Value::Real(real(&args[0]) / 0.142065),
        Mk0329 => Value::Real(real(&args[0]) * 0.142065),
        Mk0330 => Value::Real(real(&args[0]) / 0.142065),
        Mk0331 => Value::Real(real(&args[0]) * 1e-06),
        Mk0332 => Value::Real(real(&args[0]) / 1e-06),
        Mk0333 => Value::Real(real(&args[0]) * 0.0001),
        Mk0334 => Value::Real(real(&args[0]) / 0.0001),
        Mk0335 => Value::Real(real(&args[0]) * 0.01),
        Mk0336 => Value::Real(real(&args[0]) / 0.01),
        Mk0337 => Value::Real(real(&args[0]) * 1.0),
        Mk0338 => Value::Real(real(&args[0]) / 1.0),
        Mk0339 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0340 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0341 => Value::Real(real(&args[0]) * 0.00064516),
        Mk0342 => Value::Real(real(&args[0]) / 0.00064516),
        Mk0343 => Value::Real(real(&args[0]) * 0.09290304),
        Mk0344 => Value::Real(real(&args[0]) / 0.09290304),
        Mk0345 => Value::Real(real(&args[0]) * 0.83612736),
        Mk0346 => Value::Real(real(&args[0]) / 0.83612736),
        Mk0347 => Value::Real(real(&args[0]) * 2589988.110336),
        Mk0348 => Value::Real(real(&args[0]) / 2589988.110336),
        Mk0349 => Value::Real(real(&args[0]) * 4046.8564224),
        Mk0350 => Value::Real(real(&args[0]) / 4046.8564224),
        Mk0351 => Value::Real(real(&args[0]) * 10000.0),
        Mk0352 => Value::Real(real(&args[0]) / 10000.0),
        Mk0353 => Value::Real(real(&args[0]) * 100.0),
        Mk0354 => Value::Real(real(&args[0]) / 100.0),
        Mk0355 => Value::Real(real(&args[0]) * 1.0),
        Mk0356 => Value::Real(real(&args[0]) / 1.0),
        Mk0357 => Value::Real(real(&args[0]) * 1011.7141056),
        Mk0358 => Value::Real(real(&args[0]) / 1011.7141056),
        Mk0359 => Value::Real(real(&args[0]) * 25.29285264),
        Mk0360 => Value::Real(real(&args[0]) / 25.29285264),
        Mk0361 => Value::Real(real(&args[0]) * 404.68564224),
        Mk0362 => Value::Real(real(&args[0]) / 404.68564224),
        Mk0363 => Value::Real(real(&args[0]) * 1e-28),
        Mk0364 => Value::Real(real(&args[0]) / 1e-28),
        Mk0365 => Value::Real(real(&args[0]) * 93239571.972),
        Mk0366 => Value::Real(real(&args[0]) / 93239571.972),
        Mk0367 => Value::Real(real(&args[0]) * 2589988.110336),
        Mk0368 => Value::Real(real(&args[0]) / 2589988.110336),
        Mk0369 => Value::Real(real(&args[0]) * 3429904.0),
        Mk0370 => Value::Real(real(&args[0]) / 3429904.0),
        Mk0371 => Value::Real(real(&args[0]) * 1000.0),
        Mk0372 => Value::Real(real(&args[0]) / 1000.0),
        Mk0373 => Value::Real(real(&args[0]) * 25.29285264),
        Mk0374 => Value::Real(real(&args[0]) / 25.29285264),
        Mk0375 => Value::Real(real(&args[0]) * 0.04046856),
        Mk0376 => Value::Real(real(&args[0]) / 0.04046856),
        Mk0377 => Value::Real(real(&args[0]) * 5.067075e-10),
        Mk0378 => Value::Real(real(&args[0]) / 5.067075e-10),
        Mk0379 => Value::Real(real(&args[0]) * 3930.395625),
        Mk0380 => Value::Real(real(&args[0]) / 3930.395625),
        Mk0381 => Value::Real(real(&args[0]) * 2500.0),
        Mk0382 => Value::Real(real(&args[0]) / 2500.0),
        Mk0383 => Value::Real(real(&args[0]) * 486000.0),
        Mk0384 => Value::Real(real(&args[0]) / 486000.0),
        Mk0385 => Value::Real(real(&args[0]) * 121406.0),
        Mk0386 => Value::Real(real(&args[0]) / 121406.0),
        Mk0387 => Value::Real(real(&args[0]) * 485600.0),
        Mk0388 => Value::Real(real(&args[0]) / 485600.0),
        Mk0389 => Value::Real(real(&args[0]) * 0.001),
        Mk0390 => Value::Real(real(&args[0]) / 0.001),
        Mk0391 => Value::Real(real(&args[0]) * 1e-06),
        Mk0392 => Value::Real(real(&args[0]) / 1e-06),
        Mk0393 => Value::Real(real(&args[0]) * 1e-09),
        Mk0394 => Value::Real(real(&args[0]) / 1e-09),
        Mk0395 => Value::Real(real(&args[0]) * 1e-12),
        Mk0396 => Value::Real(real(&args[0]) / 1e-12),
        Mk0397 => Value::Real(real(&args[0]) * 60.0),
        Mk0398 => Value::Real(real(&args[0]) / 60.0),
        Mk0399 => Value::Real(real(&args[0]) * 3600.0),
        Mk0400 => Value::Real(real(&args[0]) / 3600.0),
        Mk0401 => Value::Real(real(&args[0]) * 86400.0),
        Mk0402 => Value::Real(real(&args[0]) / 86400.0),
        Mk0403 => Value::Real(real(&args[0]) * 604800.0),
        Mk0404 => Value::Real(real(&args[0]) / 604800.0),
        Mk0405 => Value::Real(real(&args[0]) * 1209600.0),
        Mk0406 => Value::Real(real(&args[0]) / 1209600.0),
        Mk0407 => Value::Real(real(&args[0]) * 2629746.0),
        Mk0408 => Value::Real(real(&args[0]) / 2629746.0),
        Mk0409 => Value::Real(real(&args[0]) * 31557600.0),
        Mk0410 => Value::Real(real(&args[0]) / 31557600.0),
        Mk0411 => Value::Real(real(&args[0]) * 31536000.0),
        Mk0412 => Value::Real(real(&args[0]) / 31536000.0),
        Mk0413 => Value::Real(real(&args[0]) * 315360000.0),
        Mk0414 => Value::Real(real(&args[0]) / 315360000.0),
        Mk0415 => Value::Real(real(&args[0]) * 3153600000.0),
        Mk0416 => Value::Real(real(&args[0]) / 3153600000.0),
        Mk0417 => Value::Real(real(&args[0]) * 31536000000.0),
        Mk0418 => Value::Real(real(&args[0]) / 31536000000.0),
        Mk0419 => Value::Real(real(&args[0]) * 3e-24),
        Mk0420 => Value::Real(real(&args[0]) / 3e-24),
        Mk0421 => Value::Real(real(&args[0]) * 1e-08),
        Mk0422 => Value::Real(real(&args[0]) / 1e-08),
        Mk0423 => Value::Real(real(&args[0]) * 5.391247e-44),
        Mk0424 => Value::Real(real(&args[0]) / 5.391247e-44),
        Mk0425 => Value::Real(real(&args[0]) * 88775.244),
        Mk0426 => Value::Real(real(&args[0]) / 88775.244),
        Mk0427 => Value::Real(real(&args[0]) * 157680000.0),
        Mk0428 => Value::Real(real(&args[0]) / 157680000.0),
        Mk0429 => Value::Real(real(&args[0]) * 126230400.0),
        Mk0430 => Value::Real(real(&args[0]) / 126230400.0),
        Mk0431 => Value::Real(real(&args[0]) * 864.0),
        Mk0432 => Value::Real(real(&args[0]) / 864.0),
        Mk0433 => Value::Real(real(&args[0]) * 1440.0),
        Mk0434 => Value::Real(real(&args[0]) / 1440.0),
        Mk0435 => Value::Real(real(&args[0]) * 14400.0),
        Mk0436 => Value::Real(real(&args[0]) / 14400.0),
        Mk0437 => Value::Real(real(&args[0]) * 0.1),
        Mk0438 => Value::Real(real(&args[0]) / 0.1),
        Mk0439 => Value::Real(real(&args[0]) * 90.0),
        Mk0440 => Value::Real(real(&args[0]) / 90.0),
        Mk0441 => Value::Real(real(&args[0]) * 86164.0905),
        Mk0442 => Value::Real(real(&args[0]) / 86164.0905),
        Mk0443 => Value::Real(real(&args[0]) * 31558149.504),
        Mk0444 => Value::Real(real(&args[0]) / 31558149.504),
        Mk0445 => Value::Real(real(&args[0]) * 31558432.55),
        Mk0446 => Value::Real(real(&args[0]) / 31558432.55),
        Mk0447 => Value::Real(real(&args[0]) * 0.277777778),
        Mk0448 => Value::Real(real(&args[0]) / 0.277777778),
        Mk0449 => Value::Real(real(&args[0]) * 0.44704),
        Mk0450 => Value::Real(real(&args[0]) / 0.44704),
        Mk0451 => Value::Real(real(&args[0]) * 0.514444444),
        Mk0452 => Value::Real(real(&args[0]) / 0.514444444),
        Mk0453 => Value::Real(real(&args[0]) * 0.3048),
        Mk0454 => Value::Real(real(&args[0]) / 0.3048),
        Mk0455 => Value::Real(real(&args[0]) * 340.29),
        Mk0456 => Value::Real(real(&args[0]) / 340.29),
        Mk0457 => Value::Real(real(&args[0]) * 299792458.0),
        Mk0458 => Value::Real(real(&args[0]) / 299792458.0),
        Mk0459 => Value::Real(real(&args[0]) * 0.0254),
        Mk0460 => Value::Real(real(&args[0]) / 0.0254),
        Mk0461 => Value::Real(real(&args[0]) * 1000.0),
        Mk0462 => Value::Real(real(&args[0]) / 1000.0),
        Mk0463 => Value::Real(real(&args[0]) * 1609.344),
        Mk0464 => Value::Real(real(&args[0]) / 1609.344),
        Mk0465 => Value::Real(real(&args[0]) * 0.00508),
        Mk0466 => Value::Real(real(&args[0]) / 0.00508),
        Mk0467 => Value::Real(real(&args[0]) * 0.0166666667),
        Mk0468 => Value::Real(real(&args[0]) / 0.0166666667),
        Mk0469 => Value::Real(real(&args[0]) * 0.000166309),
        Mk0470 => Value::Real(real(&args[0]) / 0.000166309),
        Mk0471 => Value::Real(real(&args[0]) * 0.836),
        Mk0472 => Value::Real(real(&args[0]) / 0.836),
        Mk0473 => Value::Real(real(&args[0]) * 0.9144),
        Mk0474 => Value::Real(real(&args[0]) / 0.9144),
        Mk0475 => Value::Real(real(&args[0]) * 0.296),
        Mk0476 => Value::Real(real(&args[0]) / 0.296),
        Mk0477 => Value::Real(real(&args[0]) * 0.74),
        Mk0478 => Value::Real(real(&args[0]) / 0.74),
        Mk0479 => Value::Real(real(&args[0]) * 1.48),
        Mk0480 => Value::Real(real(&args[0]) / 1.48),
        Mk0481 => Value::Real(real(&args[0]) * 35.5),
        Mk0482 => Value::Real(real(&args[0]) / 35.5),
        Mk0483 => Value::Real(real(&args[0]) * 1480.0),
        Mk0484 => Value::Real(real(&args[0]) / 1480.0),
        Mk0485 => Value::Real(real(&args[0]) * 1066.8),
        Mk0486 => Value::Real(real(&args[0]) / 1066.8),
        Mk0487 => Value::Real(real(&args[0]) * 0.7112),
        Mk0488 => Value::Real(real(&args[0]) / 0.7112),
        Mk0489 => Value::Real(real(&args[0]) * 2.1336),
        Mk0490 => Value::Real(real(&args[0]) / 2.1336),
        Mk0491 => Value::Real(real(&args[0]) * 0.04445),
        Mk0492 => Value::Real(real(&args[0]) / 0.04445),
        Mk0493 => Value::Real(real(&args[0]) * 500.0),
        Mk0494 => Value::Real(real(&args[0]) / 500.0),
        Mk0495 => Value::Real(real(&args[0]) * 3.333),
        Mk0496 => Value::Real(real(&args[0]) / 3.333),
        Mk0497 => Value::Real(real(&args[0]) * 0.3333),
        Mk0498 => Value::Real(real(&args[0]) / 0.3333),
        Mk0499 => Value::Real(real(&args[0]) * 0.03333),
        Mk0500 => Value::Real(real(&args[0]) / 0.03333),
        Mk0501 => Value::Real(real(&args[0]) * 3927.0),
        Mk0502 => Value::Real(real(&args[0]) / 3927.0),
        Mk0503 => Value::Real(real(&args[0]) * 1.818),
        Mk0504 => Value::Real(real(&args[0]) / 1.818),
        Mk0505 => Value::Real(real(&args[0]) * 0.303),
        Mk0506 => Value::Real(real(&args[0]) / 0.303),
        Mk0507 => Value::Real(real(&args[0]) * 0.0303),
        Mk0508 => Value::Real(real(&args[0]) / 0.0303),
        Mk0509 => Value::Real(real(&args[0]) * 2.0),
        Mk0510 => Value::Real(real(&args[0]) / 2.0),
        Mk0511 => Value::Real(real(&args[0]) * 0.6),
        Mk0512 => Value::Real(real(&args[0]) / 0.6),
        Mk0513 => Value::Real(real(&args[0]) * 2.0),
        Mk0514 => Value::Real(real(&args[0]) / 2.0),
        Mk0515 => Value::Real(real(&args[0]) * 1.949),
        Mk0516 => Value::Real(real(&args[0]) / 1.949),
        Mk0517 => Value::Real(real(&args[0]) * 1.188),
        Mk0518 => Value::Real(real(&args[0]) / 1.188),
        Mk0519 => Value::Real(real(&args[0]) * 0.3248),
        Mk0520 => Value::Real(real(&args[0]) / 0.3248),
        Mk0521 => Value::Real(real(&args[0]) * 0.8359),
        Mk0522 => Value::Real(real(&args[0]) / 0.8359),
        Mk0523 => Value::Real(real(&args[0]) * 4180.0),
        Mk0524 => Value::Real(real(&args[0]) / 4180.0),
        Mk0525 => Value::Real(real(&args[0]) * 0.3289),
        Mk0526 => Value::Real(real(&args[0]) / 0.3289),
        Mk0527 => Value::Real(real(&args[0]) * 0.02741),
        Mk0528 => Value::Real(real(&args[0]) / 0.02741),
        Mk0529 => Value::Real(real(&args[0]) * 0.6048),
        Mk0530 => Value::Real(real(&args[0]) / 0.6048),
        Mk0531 => Value::Real(real(&args[0]) * 0.0378),
        Mk0532 => Value::Real(real(&args[0]) / 0.0378),
        Mk0533 => Value::Real(real(&args[0]) * 0.00375),
        Mk0534 => Value::Real(real(&args[0]) / 0.00375),
        Mk0535 => Value::Real(real(&args[0]) * 60.48),
        Mk0536 => Value::Real(real(&args[0]) / 60.48),
        Mk0537 => Value::Real(real(&args[0]) * 37.3242),
        Mk0538 => Value::Real(real(&args[0]) / 37.3242),
        Mk0539 => Value::Real(real(&args[0]) * 0.933),
        Mk0540 => Value::Real(real(&args[0]) / 0.933),
        Mk0541 => Value::Real(real(&args[0]) * 0.011664),
        Mk0542 => Value::Real(real(&args[0]) / 0.011664),
        Mk0543 => Value::Real(real(&args[0]) * 254.0),
        Mk0544 => Value::Real(real(&args[0]) / 254.0),
        Mk0545 => Value::Real(real(&args[0]) * 1.2829),
        Mk0546 => Value::Real(real(&args[0]) / 1.2829),
        Mk0547 => Value::Real(real(&args[0]) * 0.4082),
        Mk0548 => Value::Real(real(&args[0]) / 0.4082),
        Mk0549 => Value::Real(real(&args[0]) * 16.38),
        Mk0550 => Value::Real(real(&args[0]) / 16.38),
        Mk0551 => Value::Real(real(&args[0]) * 0.004266),
        Mk0552 => Value::Real(real(&args[0]) / 0.004266),
        Mk0553 => Value::Real(real(&args[0]) * 0.4095),
        Mk0554 => Value::Real(real(&args[0]) / 0.4095),
        Mk0555 => Value::Real(real(&args[0]) * 180.39),
        Mk0556 => Value::Real(real(&args[0]) / 180.39),
        Mk0557 => Value::Real(real(&args[0]) * 18.039),
        Mk0558 => Value::Real(real(&args[0]) / 18.039),
        Mk0559 => Value::Real(real(&args[0]) * 1.8039),
        Mk0560 => Value::Real(real(&args[0]) / 1.8039),
        Mk0561 => Value::Real(real(&args[0]) * 0.18039),
        Mk0562 => Value::Real(real(&args[0]) / 0.18039),
        Mk0563 => Value::Real(real(&args[0]) * 26.2),
        Mk0564 => Value::Real(real(&args[0]) / 26.2),
        Mk0565 => Value::Real(real(&args[0]) * 3.27),
        Mk0566 => Value::Real(real(&args[0]) / 3.27),
        Mk0567 => Value::Real(real(&args[0]) * 0.546),
        Mk0568 => Value::Real(real(&args[0]) / 0.546),
        Mk0569 => Value::Real(real(&args[0]) * 0.0455),
        Mk0570 => Value::Real(real(&args[0]) / 0.0455),
        Mk0571 => Value::Real(real(&args[0]) * 8.73),
        Mk0572 => Value::Real(real(&args[0]) / 8.73),
        Mk0573 => Value::Real(real(&args[0]) * 13.1),
        Mk0574 => Value::Real(real(&args[0]) / 13.1),
        Mk0575 => Value::Real(real(&args[0]) * 1.2829),
        Mk0576 => Value::Real(real(&args[0]) / 1.2829),
        Mk0577 => Value::Real(real(&args[0]) * 7.7),
        Mk0578 => Value::Real(real(&args[0]) / 7.7),
        Mk0579 => Value::Real(real(&args[0]) * 3.6),
        Mk0580 => Value::Real(real(&args[0]) / 3.6),
        Mk0581 => Value::Real(real(&args[0]) * 2.2),
        Mk0582 => Value::Real(real(&args[0]) / 2.2),
        Mk0583 => Value::Real(real(&args[0]) * 22.0),
        Mk0584 => Value::Real(real(&args[0]) / 22.0),
        Mk0585 => Value::Real(real(&args[0]) * 1.62),
        Mk0586 => Value::Real(real(&args[0]) / 1.62),
        Mk0587 => Value::Real(real(&args[0]) * 3.71),
        Mk0588 => Value::Real(real(&args[0]) / 3.71),
        Mk0589 => Value::Real(real(&args[0]) * 24.79),
        Mk0590 => Value::Real(real(&args[0]) / 24.79),
        Mk0591 => Value::Real(real(&args[0]) * 8.87),
        Mk0592 => Value::Real(real(&args[0]) / 8.87),
        Mk0593 => Value::Real(real(&args[0]) * 3.7),
        Mk0594 => Value::Real(real(&args[0]) / 3.7),
        Mk0595 => Value::Real(real(&args[0]) * 10.44),
        Mk0596 => Value::Real(real(&args[0]) / 10.44),
        Mk0597 => Value::Real(real(&args[0]) * 8.69),
        Mk0598 => Value::Real(real(&args[0]) / 8.69),
        Mk0599 => Value::Real(real(&args[0]) * 11.15),
        Mk0600 => Value::Real(real(&args[0]) / 11.15),
        Mk0601 => Value::Real(real(&args[0]) * 274.0),
        Mk0602 => Value::Real(real(&args[0]) / 274.0),
        Mk0603 => Value::Real(real(&args[0]) * 0.62),
        Mk0604 => Value::Real(real(&args[0]) / 0.62),
        Mk0605 => Value::Real(real(&args[0]) * 0.27),
        Mk0606 => Value::Real(real(&args[0]) / 0.27),
        Mk0607 => Value::Real(real(&args[0]) * 1.35),
        Mk0608 => Value::Real(real(&args[0]) / 1.35),
        Mk0609 => Value::Real(real(&args[0]) * 1.31),
        Mk0610 => Value::Real(real(&args[0]) / 1.31),
        Mk0611 => Value::Real(real(&args[0]) * 1.8),
        Mk0612 => Value::Real(real(&args[0]) / 1.8),
        Mk0613 => Value::Real(real(&args[0]) * 1.43),
        Mk0614 => Value::Real(real(&args[0]) / 1.43),
        Mk0615 => Value::Real(real(&args[0]) * 1000.0),
        Mk0616 => Value::Real(real(&args[0]) / 1000.0),
        Mk0617 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0618 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0619 => Value::Real(real(&args[0]) * 1000000000.0),
        Mk0620 => Value::Real(real(&args[0]) / 1000000000.0),
        Mk0621 => Value::Real(real(&args[0]) * 100.0),
        Mk0622 => Value::Real(real(&args[0]) / 100.0),
        Mk0623 => Value::Real(real(&args[0]) * 100000.0),
        Mk0624 => Value::Real(real(&args[0]) / 100000.0),
        Mk0625 => Value::Real(real(&args[0]) * 100.0),
        Mk0626 => Value::Real(real(&args[0]) / 100.0),
        Mk0627 => Value::Real(real(&args[0]) * 0.1),
        Mk0628 => Value::Real(real(&args[0]) / 0.1),
        Mk0629 => Value::Real(real(&args[0]) * 101325.0),
        Mk0630 => Value::Real(real(&args[0]) / 101325.0),
        Mk0631 => Value::Real(real(&args[0]) * 133.322368),
        Mk0632 => Value::Real(real(&args[0]) / 133.322368),
        Mk0633 => Value::Real(real(&args[0]) * 133.322387415),
        Mk0634 => Value::Real(real(&args[0]) / 133.322387415),
        Mk0635 => Value::Real(real(&args[0]) * 3386.389),
        Mk0636 => Value::Real(real(&args[0]) / 3386.389),
        Mk0637 => Value::Real(real(&args[0]) * 6894.757293),
        Mk0638 => Value::Real(real(&args[0]) / 6894.757293),
        Mk0639 => Value::Real(real(&args[0]) * 6894757.293),
        Mk0640 => Value::Real(real(&args[0]) / 6894757.293),
        Mk0641 => Value::Real(real(&args[0]) * 9.80665),
        Mk0642 => Value::Real(real(&args[0]) / 9.80665),
        Mk0643 => Value::Real(real(&args[0]) * 249.0889),
        Mk0644 => Value::Real(real(&args[0]) / 249.0889),
        Mk0645 => Value::Real(real(&args[0]) * 0.1),
        Mk0646 => Value::Real(real(&args[0]) / 0.1),
        Mk0647 => Value::Real(real(&args[0]) * 1000.0),
        Mk0648 => Value::Real(real(&args[0]) / 1000.0),
        Mk0649 => Value::Real(real(&args[0]) * 98066.5),
        Mk0650 => Value::Real(real(&args[0]) / 98066.5),
        Mk0651 => Value::Real(real(&args[0]) * 98066.5),
        Mk0652 => Value::Real(real(&args[0]) / 98066.5),
        Mk0653 => Value::Real(real(&args[0]) * 0.1),
        Mk0654 => Value::Real(real(&args[0]) / 0.1),
        Mk0655 => Value::Real(real(&args[0]) * 2989.067),
        Mk0656 => Value::Real(real(&args[0]) / 2989.067),
        Mk0657 => Value::Real(real(&args[0]) * 1.448164),
        Mk0658 => Value::Real(real(&args[0]) / 1.448164),
        Mk0659 => Value::Real(real(&args[0]) * 1000.0),
        Mk0660 => Value::Real(real(&args[0]) / 1000.0),
        Mk0661 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0662 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0663 => Value::Real(real(&args[0]) * 1000000000.0),
        Mk0664 => Value::Real(real(&args[0]) / 1000000000.0),
        Mk0665 => Value::Real(real(&args[0]) * 0.001),
        Mk0666 => Value::Real(real(&args[0]) / 0.001),
        Mk0667 => Value::Real(real(&args[0]) * 1e-06),
        Mk0668 => Value::Real(real(&args[0]) / 1e-06),
        Mk0669 => Value::Real(real(&args[0]) * 4.184),
        Mk0670 => Value::Real(real(&args[0]) / 4.184),
        Mk0671 => Value::Real(real(&args[0]) * 4184.0),
        Mk0672 => Value::Real(real(&args[0]) / 4184.0),
        Mk0673 => Value::Real(real(&args[0]) * 3600.0),
        Mk0674 => Value::Real(real(&args[0]) / 3600.0),
        Mk0675 => Value::Real(real(&args[0]) * 3600000.0),
        Mk0676 => Value::Real(real(&args[0]) / 3600000.0),
        Mk0677 => Value::Real(real(&args[0]) * 3600000000.0),
        Mk0678 => Value::Real(real(&args[0]) / 3600000000.0),
        Mk0679 => Value::Real(real(&args[0]) * 1.602176634e-19),
        Mk0680 => Value::Real(real(&args[0]) / 1.602176634e-19),
        Mk0681 => Value::Real(real(&args[0]) * 1.602176634e-16),
        Mk0682 => Value::Real(real(&args[0]) / 1.602176634e-16),
        Mk0683 => Value::Real(real(&args[0]) * 1.602176634e-13),
        Mk0684 => Value::Real(real(&args[0]) / 1.602176634e-13),
        Mk0685 => Value::Real(real(&args[0]) * 1e-07),
        Mk0686 => Value::Real(real(&args[0]) / 1e-07),
        Mk0687 => Value::Real(real(&args[0]) * 1055.05585),
        Mk0688 => Value::Real(real(&args[0]) / 1055.05585),
        Mk0689 => Value::Real(real(&args[0]) * 105506000.0),
        Mk0690 => Value::Real(real(&args[0]) / 105506000.0),
        Mk0691 => Value::Real(real(&args[0]) * 1.3558179483),
        Mk0692 => Value::Real(real(&args[0]) / 1.3558179483),
        Mk0693 => Value::Real(real(&args[0]) * 0.04214011),
        Mk0694 => Value::Real(real(&args[0]) / 0.04214011),
        Mk0695 => Value::Real(real(&args[0]) * 4184000000.0),
        Mk0696 => Value::Real(real(&args[0]) / 4184000000.0),
        Mk0697 => Value::Real(real(&args[0]) * 4184000000000.0),
        Mk0698 => Value::Real(real(&args[0]) / 4184000000000.0),
        Mk0699 => Value::Real(real(&args[0]) * 4184000000000000.0),
        Mk0700 => Value::Real(real(&args[0]) / 4184000000000000.0),
        Mk0701 => Value::Real(real(&args[0]) * 4.3597447222071e-18),
        Mk0702 => Value::Real(real(&args[0]) / 4.3597447222071e-18),
        Mk0703 => Value::Real(real(&args[0]) * 2.1798723611035e-18),
        Mk0704 => Value::Real(real(&args[0]) / 2.1798723611035e-18),
        Mk0705 => Value::Real(real(&args[0]) * 1.05506e+18),
        Mk0706 => Value::Real(real(&args[0]) / 1.05506e+18),
        Mk0707 => Value::Real(real(&args[0]) * 2684519.537),
        Mk0708 => Value::Real(real(&args[0]) / 2684519.537),
        Mk0709 => Value::Real(real(&args[0]) * 101.325),
        Mk0710 => Value::Real(real(&args[0]) / 101.325),
        Mk0711 => Value::Real(real(&args[0]) * 1899.1),
        Mk0712 => Value::Real(real(&args[0]) / 1899.1),
        Mk0713 => Value::Real(real(&args[0]) * 1000.0),
        Mk0714 => Value::Real(real(&args[0]) / 1000.0),
        Mk0715 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0716 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0717 => Value::Real(real(&args[0]) * 1000000000.0),
        Mk0718 => Value::Real(real(&args[0]) / 1000000000.0),
        Mk0719 => Value::Real(real(&args[0]) * 0.001),
        Mk0720 => Value::Real(real(&args[0]) / 0.001),
        Mk0721 => Value::Real(real(&args[0]) * 1e-06),
        Mk0722 => Value::Real(real(&args[0]) / 1e-06),
        Mk0723 => Value::Real(real(&args[0]) * 745.699872),
        Mk0724 => Value::Real(real(&args[0]) / 745.699872),
        Mk0725 => Value::Real(real(&args[0]) * 735.49875),
        Mk0726 => Value::Real(real(&args[0]) / 735.49875),
        Mk0727 => Value::Real(real(&args[0]) * 746.0),
        Mk0728 => Value::Real(real(&args[0]) / 746.0),
        Mk0729 => Value::Real(real(&args[0]) * 0.29307107),
        Mk0730 => Value::Real(real(&args[0]) / 0.29307107),
        Mk0731 => Value::Real(real(&args[0]) * 4.184),
        Mk0732 => Value::Real(real(&args[0]) / 4.184),
        Mk0733 => Value::Real(real(&args[0]) * 1e-07),
        Mk0734 => Value::Real(real(&args[0]) / 1e-07),
        Mk0735 => Value::Real(real(&args[0]) * 1.3558179483),
        Mk0736 => Value::Real(real(&args[0]) / 1.3558179483),
        Mk0737 => Value::Real(real(&args[0]) * 3516.853),
        Mk0738 => Value::Real(real(&args[0]) / 3516.853),
        Mk0739 => Value::Real(real(&args[0]) * 1.0),
        Mk0740 => Value::Real(real(&args[0]) / 1.0),
        Mk0741 => Value::Real(real(&args[0]) * 1000.0),
        Mk0742 => Value::Real(real(&args[0]) / 1000.0),
        Mk0743 => Value::Real(real(&args[0]) * 735.49875),
        Mk0744 => Value::Real(real(&args[0]) / 735.49875),
        Mk0745 => Value::Real(real(&args[0]) * 980.665),
        Mk0746 => Value::Real(real(&args[0]) / 980.665),
        Mk0747 => Value::Real(real(&args[0]) * 0.0001333),
        Mk0748 => Value::Real(real(&args[0]) / 0.0001333),
        Mk0749 => Value::Real(real(&args[0]) * 1000.0),
        Mk0750 => Value::Real(real(&args[0]) / 1000.0),
        Mk0751 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0752 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0753 => Value::Real(real(&args[0]) * 0.001),
        Mk0754 => Value::Real(real(&args[0]) / 0.001),
        Mk0755 => Value::Real(real(&args[0]) * 1e-05),
        Mk0756 => Value::Real(real(&args[0]) / 1e-05),
        Mk0757 => Value::Real(real(&args[0]) * 9.80665),
        Mk0758 => Value::Real(real(&args[0]) / 9.80665),
        Mk0759 => Value::Real(real(&args[0]) * 0.00980665),
        Mk0760 => Value::Real(real(&args[0]) / 0.00980665),
        Mk0761 => Value::Real(real(&args[0]) * 4.4482216153),
        Mk0762 => Value::Real(real(&args[0]) / 4.4482216153),
        Mk0763 => Value::Real(real(&args[0]) * 0.278013851),
        Mk0764 => Value::Real(real(&args[0]) / 0.278013851),
        Mk0765 => Value::Real(real(&args[0]) * 0.138254954),
        Mk0766 => Value::Real(real(&args[0]) / 0.138254954),
        Mk0767 => Value::Real(real(&args[0]) * 4448.2216153),
        Mk0768 => Value::Real(real(&args[0]) / 4448.2216153),
        Mk0769 => Value::Real(real(&args[0]) * 9806.65),
        Mk0770 => Value::Real(real(&args[0]) / 9806.65),
        Mk0771 => Value::Real(real(&args[0]) * 1000.0),
        Mk0772 => Value::Real(real(&args[0]) / 1000.0),
        Mk0773 => Value::Real(real(&args[0]) * 0.1),
        Mk0774 => Value::Real(real(&args[0]) / 0.1),
        Mk0775 => Value::Real(real(&args[0]) * 0.125),
        Mk0776 => Value::Real(real(&args[0]) / 0.125),
        Mk0777 => Value::Real(real(&args[0]) * 125.0),
        Mk0778 => Value::Real(real(&args[0]) / 125.0),
        Mk0779 => Value::Real(real(&args[0]) * 125000.0),
        Mk0780 => Value::Real(real(&args[0]) / 125000.0),
        Mk0781 => Value::Real(real(&args[0]) * 125000000.0),
        Mk0782 => Value::Real(real(&args[0]) / 125000000.0),
        Mk0783 => Value::Real(real(&args[0]) * 1000.0),
        Mk0784 => Value::Real(real(&args[0]) / 1000.0),
        Mk0785 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0786 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0787 => Value::Real(real(&args[0]) * 1000000000.0),
        Mk0788 => Value::Real(real(&args[0]) / 1000000000.0),
        Mk0789 => Value::Real(real(&args[0]) * 1000000000000.0),
        Mk0790 => Value::Real(real(&args[0]) / 1000000000000.0),
        Mk0791 => Value::Real(real(&args[0]) * 1000000000000000.0),
        Mk0792 => Value::Real(real(&args[0]) / 1000000000000000.0),
        Mk0793 => Value::Real(real(&args[0]) * 1e+18),
        Mk0794 => Value::Real(real(&args[0]) / 1e+18),
        Mk0795 => Value::Real(real(&args[0]) * 1024.0),
        Mk0796 => Value::Real(real(&args[0]) / 1024.0),
        Mk0797 => Value::Real(real(&args[0]) * 1048576.0),
        Mk0798 => Value::Real(real(&args[0]) / 1048576.0),
        Mk0799 => Value::Real(real(&args[0]) * 1073741824.0),
        Mk0800 => Value::Real(real(&args[0]) / 1073741824.0),
        Mk0801 => Value::Real(real(&args[0]) * 1099511627776.0),
        Mk0802 => Value::Real(real(&args[0]) / 1099511627776.0),
        Mk0803 => Value::Real(real(&args[0]) * 1125899906842624.0),
        Mk0804 => Value::Real(real(&args[0]) / 1125899906842624.0),
        Mk0805 => Value::Real(real(&args[0]) * 128.0),
        Mk0806 => Value::Real(real(&args[0]) / 128.0),
        Mk0807 => Value::Real(real(&args[0]) * 131072.0),
        Mk0808 => Value::Real(real(&args[0]) / 131072.0),
        Mk0809 => Value::Real(real(&args[0]) * 2.0),
        Mk0810 => Value::Real(real(&args[0]) / 2.0),
        Mk0811 => Value::Real(real(&args[0]) * 4.0),
        Mk0812 => Value::Real(real(&args[0]) / 4.0),
        Mk0813 => Value::Real(real(&args[0]) * 8.0),
        Mk0814 => Value::Real(real(&args[0]) / 8.0),
        Mk0815 => Value::Real(real(&args[0]) * 1.0),
        Mk0816 => Value::Real(real(&args[0]) / 1.0),
        Mk0817 => Value::Real(real(&args[0]) * 0.25),
        Mk0818 => Value::Real(real(&args[0]) / 0.25),
        Mk0819 => Value::Real(real(&args[0]) * 57.29577951308232),
        Mk0820 => Value::Real(real(&args[0]) / 57.29577951308232),
        Mk0821 => Value::Real(real(&args[0]) * 0.9),
        Mk0822 => Value::Real(real(&args[0]) / 0.9),
        Mk0823 => Value::Real(real(&args[0]) * 0.016666667),
        Mk0824 => Value::Real(real(&args[0]) / 0.016666667),
        Mk0825 => Value::Real(real(&args[0]) * 0.000277778),
        Mk0826 => Value::Real(real(&args[0]) / 0.000277778),
        Mk0827 => Value::Real(real(&args[0]) * 0.05729578),
        Mk0828 => Value::Real(real(&args[0]) / 0.05729578),
        Mk0829 => Value::Real(real(&args[0]) * 360.0),
        Mk0830 => Value::Real(real(&args[0]) / 360.0),
        Mk0831 => Value::Real(real(&args[0]) * 90.0),
        Mk0832 => Value::Real(real(&args[0]) / 90.0),
        Mk0833 => Value::Real(real(&args[0]) * 60.0),
        Mk0834 => Value::Real(real(&args[0]) / 60.0),
        Mk0835 => Value::Real(real(&args[0]) * 45.0),
        Mk0836 => Value::Real(real(&args[0]) / 45.0),
        Mk0837 => Value::Real(real(&args[0]) * 30.0),
        Mk0838 => Value::Real(real(&args[0]) / 30.0),
        Mk0839 => Value::Real(real(&args[0]) * 11.25),
        Mk0840 => Value::Real(real(&args[0]) / 11.25),
        Mk0841 => Value::Real(real(&args[0]) * 0.05625),
        Mk0842 => Value::Real(real(&args[0]) / 0.05625),
        Mk0843 => Value::Real(real(&args[0]) * 0.06),
        Mk0844 => Value::Real(real(&args[0]) / 0.06),
        Mk0845 => Value::Real(real(&args[0]) * 15.0),
        Mk0846 => Value::Real(real(&args[0]) / 15.0),
        Mk0847 => Value::Real(real(&args[0]) * 0.009),
        Mk0848 => Value::Real(real(&args[0]) / 0.009),
        Mk0849 => Value::Real(real(&args[0]) * 9e-05),
        Mk0850 => Value::Real(real(&args[0]) / 9e-05),
        Mk0851 => Value::Real(real(&args[0]) * 1.40625),
        Mk0852 => Value::Real(real(&args[0]) / 1.40625),
        Mk0853 => Value::Real(real(&args[0]) * 1000.0),
        Mk0854 => Value::Real(real(&args[0]) / 1000.0),
        Mk0855 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0856 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0857 => Value::Real(real(&args[0]) * 1000000000.0),
        Mk0858 => Value::Real(real(&args[0]) / 1000000000.0),
        Mk0859 => Value::Real(real(&args[0]) * 1000000000000.0),
        Mk0860 => Value::Real(real(&args[0]) / 1000000000000.0),
        Mk0861 => Value::Real(real(&args[0]) * 0.001),
        Mk0862 => Value::Real(real(&args[0]) / 0.001),
        Mk0863 => Value::Real(real(&args[0]) * 0.0166666667),
        Mk0864 => Value::Real(real(&args[0]) / 0.0166666667),
        Mk0865 => Value::Real(real(&args[0]) * 0.159154943),
        Mk0866 => Value::Real(real(&args[0]) / 0.159154943),
        Mk0867 => Value::Real(real(&args[0]) * 1.0),
        Mk0868 => Value::Real(real(&args[0]) / 1.0),
        Mk0869 => Value::Real(real(&args[0]) * 0.0166666667),
        Mk0870 => Value::Real(real(&args[0]) / 0.0166666667),
        Mk0871 => Value::Real(real(&args[0]) * 10000.0),
        Mk0872 => Value::Real(real(&args[0]) / 10000.0),
        Mk0873 => Value::Real(real(&args[0]) * 0.001),
        Mk0874 => Value::Real(real(&args[0]) / 0.001),
        Mk0875 => Value::Real(real(&args[0]) * 10.76391),
        Mk0876 => Value::Real(real(&args[0]) / 10.76391),
        Mk0877 => Value::Real(real(&args[0]) * 1.0),
        Mk0878 => Value::Real(real(&args[0]) / 1.0),
        Mk0879 => Value::Real(real(&args[0]) * 1000.0),
        Mk0880 => Value::Real(real(&args[0]) / 1000.0),
        Mk0881 => Value::Real(real(&args[0]) * 10.0),
        Mk0882 => Value::Real(real(&args[0]) / 10.0),
        Mk0883 => Value::Real(real(&args[0]) * 0.001),
        Mk0884 => Value::Real(real(&args[0]) / 0.001),
        Mk0885 => Value::Real(real(&args[0]) * 1e-06),
        Mk0886 => Value::Real(real(&args[0]) / 1e-06),
        Mk0887 => Value::Real(real(&args[0]) * 1e-09),
        Mk0888 => Value::Real(real(&args[0]) / 1e-09),
        Mk0889 => Value::Real(real(&args[0]) * 10.0),
        Mk0890 => Value::Real(real(&args[0]) / 10.0),
        Mk0891 => Value::Real(real(&args[0]) * 3.335641e-10),
        Mk0892 => Value::Real(real(&args[0]) / 3.335641e-10),
        Mk0893 => Value::Real(real(&args[0]) * 3600.0),
        Mk0894 => Value::Real(real(&args[0]) / 3600.0),
        Mk0895 => Value::Real(real(&args[0]) * 3.6),
        Mk0896 => Value::Real(real(&args[0]) / 3.6),
        Mk0897 => Value::Real(real(&args[0]) * 96485.332),
        Mk0898 => Value::Real(real(&args[0]) / 96485.332),
        Mk0899 => Value::Real(real(&args[0]) * 1.602176634e-19),
        Mk0900 => Value::Real(real(&args[0]) / 1.602176634e-19),
        Mk0901 => Value::Real(real(&args[0]) * 3.335641e-10),
        Mk0902 => Value::Real(real(&args[0]) / 3.335641e-10),
        Mk0903 => Value::Real(real(&args[0]) * 1000.0),
        Mk0904 => Value::Real(real(&args[0]) / 1000.0),
        Mk0905 => Value::Real(real(&args[0]) * 1.0),
        Mk0906 => Value::Real(real(&args[0]) / 1.0),
        Mk0907 => Value::Real(real(&args[0]) * 1.0),
        Mk0908 => Value::Real(real(&args[0]) / 1.0),
        Mk0909 => Value::Real(real(&args[0]) * 16.01846337),
        Mk0910 => Value::Real(real(&args[0]) / 16.01846337),
        Mk0911 => Value::Real(real(&args[0]) * 27679.90471),
        Mk0912 => Value::Real(real(&args[0]) / 27679.90471),
        Mk0913 => Value::Real(real(&args[0]) * 1729.994),
        Mk0914 => Value::Real(real(&args[0]) / 1729.994),
        Mk0915 => Value::Real(real(&args[0]) * 119.8264273),
        Mk0916 => Value::Real(real(&args[0]) / 119.8264273),
        Mk0917 => Value::Real(real(&args[0]) * 99.77637266),
        Mk0918 => Value::Real(real(&args[0]) / 99.77637266),
        Mk0919 => Value::Real(real(&args[0]) * 515.3788184),
        Mk0920 => Value::Real(real(&args[0]) / 515.3788184),
        Mk0921 => Value::Real(real(&args[0]) * 1000.0),
        Mk0922 => Value::Real(real(&args[0]) / 1000.0),
        Mk0923 => Value::Real(real(&args[0]) * 0.001),
        Mk0924 => Value::Real(real(&args[0]) / 0.001),
        Mk0925 => Value::Real(real(&args[0]) * 1.6667e-05),
        Mk0926 => Value::Real(real(&args[0]) / 1.6667e-05),
        Mk0927 => Value::Real(real(&args[0]) * 2.7778e-07),
        Mk0928 => Value::Real(real(&args[0]) / 2.7778e-07),
        Mk0929 => Value::Real(real(&args[0]) * 0.00027778),
        Mk0930 => Value::Real(real(&args[0]) / 0.00027778),
        Mk0931 => Value::Real(real(&args[0]) * 6.30902e-05),
        Mk0932 => Value::Real(real(&args[0]) / 6.30902e-05),
        Mk0933 => Value::Real(real(&args[0]) * 0.000471947),
        Mk0934 => Value::Real(real(&args[0]) / 0.000471947),
        Mk0935 => Value::Real(real(&args[0]) * 0.0283168),
        Mk0936 => Value::Real(real(&args[0]) / 0.0283168),
        Mk0937 => Value::Real(real(&args[0]) * 1.84013e-06),
        Mk0938 => Value::Real(real(&args[0]) / 1.84013e-06),
        Mk0939 => Value::Real(real(&args[0]) * 0.01),
        Mk0940 => Value::Real(real(&args[0]) / 0.01),
        Mk0941 => Value::Real(real(&args[0]) * 1000.0),
        Mk0942 => Value::Real(real(&args[0]) / 1000.0),
        Mk0943 => Value::Real(real(&args[0]) * 1.3558179483),
        Mk0944 => Value::Real(real(&args[0]) / 1.3558179483),
        Mk0945 => Value::Real(real(&args[0]) * 0.1129848),
        Mk0946 => Value::Real(real(&args[0]) / 0.1129848),
        Mk0947 => Value::Real(real(&args[0]) * 0.00706155),
        Mk0948 => Value::Real(real(&args[0]) / 0.00706155),
        Mk0949 => Value::Real(real(&args[0]) * 9.80665),
        Mk0950 => Value::Real(real(&args[0]) / 9.80665),
        Mk0951 => Value::Real(real(&args[0]) * 1e-07),
        Mk0952 => Value::Real(real(&args[0]) / 1e-07),
        Mk0953 => Value::Real(real(&args[0]) * 9.80665e-05),
        Mk0954 => Value::Real(real(&args[0]) / 9.80665e-05),
        Mk0955 => Value::Real(real(&args[0]) * 0.01),
        Mk0956 => Value::Real(real(&args[0]) / 0.01),
        Mk0957 => Value::Real(real(&args[0]) * 9.80665),
        Mk0958 => Value::Real(real(&args[0]) / 9.80665),
        Mk0959 => Value::Real(real(&args[0]) * 0.3048),
        Mk0960 => Value::Real(real(&args[0]) / 0.3048),
        Mk0961 => Value::Real(real(&args[0]) * 0.0254),
        Mk0962 => Value::Real(real(&args[0]) / 0.0254),
        Mk0963 => Value::Real(real(&args[0]) * 0.277777778),
        Mk0964 => Value::Real(real(&args[0]) / 0.277777778),
        Mk0965 => Value::Real(real(&args[0]) * 1e-05),
        Mk0966 => Value::Real(real(&args[0]) / 1e-05),
        Mk0967 => Value::Real(real(&args[0]) * 0.425143707),
        Mk0968 => Value::Real(real(&args[0]) / 0.425143707),
        Mk0969 => Value::Real(real(&args[0]) * 0.354006043),
        Mk0970 => Value::Real(real(&args[0]) / 0.354006043),
        Mk0971 => Value::Real(real(&args[0]) * 100.0),
        Mk0972 => Value::Real(real(&args[0]) / 100.0),
        Mk0973 => Value::Real(real(&args[0]) * 1.0),
        Mk0974 => Value::Real(real(&args[0]) / 1.0),
        Mk0975 => Value::Real(real(&args[0]) * 1.609344),
        Mk0976 => Value::Real(real(&args[0]) / 1.609344),
        Mk0977 => Value::Real(real(&args[0]) * 0.001),
        Mk0978 => Value::Real(real(&args[0]) / 0.001),
        Mk0979 => Value::Real(real(&args[0]) * 1e-06),
        Mk0980 => Value::Real(real(&args[0]) / 1e-06),
        Mk0981 => Value::Real(real(&args[0]) * 1000.0),
        Mk0982 => Value::Real(real(&args[0]) / 1000.0),
        Mk0983 => Value::Real(real(&args[0]) * 1e-09),
        Mk0984 => Value::Real(real(&args[0]) / 1e-09),
        Mk0985 => Value::Real(real(&args[0]) * 453.59237),
        Mk0986 => Value::Real(real(&args[0]) / 453.59237),
        Mk0987 => Value::Real(real(&args[0]) * 1.0),
        Mk0988 => Value::Real(real(&args[0]) / 1.0),
        Mk0989 => Value::Real(real(&args[0]) * 1.6667e-08),
        Mk0990 => Value::Real(real(&args[0]) / 1.6667e-08),
        Mk0991 => Value::Real(real(&args[0]) * 1000.0),
        Mk0992 => Value::Real(real(&args[0]) / 1000.0),
        Mk0993 => Value::Real(real(&args[0]) * 1000000.0),
        Mk0994 => Value::Real(real(&args[0]) / 1000000.0),
        Mk0995 => Value::Real(real(&args[0]) * 1000000000.0),
        Mk0996 => Value::Real(real(&args[0]) / 1000000000.0),
        Mk0997 => Value::Real(real(&args[0]) * 37000000000.0),
        Mk0998 => Value::Real(real(&args[0]) / 37000000000.0),
        Mk0999 => Value::Real(real(&args[0]) * 37000000.0),
        Mk1000 => Value::Real(real(&args[0]) / 37000000.0),
        Mk1001 => Value::Real(real(&args[0]) * 37000.0),
        Mk1002 => Value::Real(real(&args[0]) / 37000.0),
        Mk1003 => Value::Real(real(&args[0]) * 1000000.0),
        Mk1004 => Value::Real(real(&args[0]) / 1000000.0),
        Mk1005 => Value::Real(real(&args[0]) * 1.0),
        Mk1006 => Value::Real(real(&args[0]) / 1.0),
        Mk1007 => Value::Real(real(&args[0]) * 0.01),
        Mk1008 => Value::Real(real(&args[0]) / 0.01),
        Mk1009 => Value::Real(real(&args[0]) * 0.01),
        Mk1010 => Value::Real(real(&args[0]) / 0.01),
        Mk1011 => Value::Real(real(&args[0]) * 1.0),
        Mk1012 => Value::Real(real(&args[0]) / 1.0),
        Mk1013 => Value::Real(real(&args[0]) * 0.000258),
        Mk1014 => Value::Real(real(&args[0]) / 0.000258),
        Mk1015 => Value::Real(real(&args[0]) * 1e-07),
        Mk1016 => Value::Real(real(&args[0]) / 1e-07),
        Mk1017 => Value::Real(real(&args[0]) * 0.1),
        Mk1018 => Value::Real(real(&args[0]) / 0.1),
        Mk1019 => Value::Real(real(&args[0]) * 0.001),
        Mk1020 => Value::Real(real(&args[0]) / 0.001),
        Mk1021 => Value::Real(real(&args[0]) * 0.001),
        Mk1022 => Value::Real(real(&args[0]) / 0.001),
        Mk1023 => Value::Real(real(&args[0]) * 0.0001),
        Mk1024 => Value::Real(real(&args[0]) / 0.0001),
        Mk1025 => Value::Real(real(&args[0]) * 1e-06),
        Mk1026 => Value::Real(real(&args[0]) / 1e-06),
        Mk1027 => Value::Real(real(&args[0]) * 6894.757),
        Mk1028 => Value::Real(real(&args[0]) / 6894.757),
        Mk1029 => Value::Real(real(&args[0]) * 0.0001),
        Mk1030 => Value::Real(real(&args[0]) / 0.0001),
        Mk1031 => Value::Real(real(&args[0]) * 1e-07),
        Mk1032 => Value::Real(real(&args[0]) / 1e-07),
        Mk1033 => Value::Real(real(&args[0]) * 1e-06),
        Mk1034 => Value::Real(real(&args[0]) / 1e-06),
        Mk1035 => Value::Real(real(&args[0]) * 0.001),
        Mk1036 => Value::Real(real(&args[0]) / 0.001),
        Mk1037 => Value::Real(real(&args[0]) * 1e-08),
        Mk1038 => Value::Real(real(&args[0]) / 1e-08),
        Mk1039 => Value::Real(real(&args[0]) * 1.0),
        Mk1040 => Value::Real(real(&args[0]) / 1.0),
        Mk1041 => Value::Real(real(&args[0]) * 1e-09),
        Mk1042 => Value::Real(real(&args[0]) / 1e-09),
        Mk1043 => Value::Real(real(&args[0]) * 0.375),
        Mk1044 => Value::Real(real(&args[0]) / 0.375),
        Mk1045 => Value::Real(real(&args[0]) * 4.5107),
        Mk1046 => Value::Real(real(&args[0]) / 4.5107),
        Mk1047 => Value::Real(real(&args[0]) * 1.8143),
        Mk1048 => Value::Real(real(&args[0]) / 1.8143),
        Mk1049 => Value::Real(real(&args[0]) * 4.2175),
        Mk1050 => Value::Real(real(&args[0]) / 4.2175),
        Mk1051 => Value::Real(real(&args[0]) * 2.10875),
        Mk1052 => Value::Real(real(&args[0]) / 2.10875),
        Mk1053 => Value::Real(real(&args[0]) * 10.0),
        Mk1054 => Value::Real(real(&args[0]) / 10.0),
        Mk1055 => Value::Real(real(&args[0]) * 5.0),
        Mk1056 => Value::Real(real(&args[0]) / 5.0),
        Mk1057 => Value::Real(real(&args[0]) * 0.616),
        Mk1058 => Value::Real(real(&args[0]) / 0.616),
        Mk1059 => Value::Real(real(&args[0]) * 0.308),
        Mk1060 => Value::Real(real(&args[0]) / 0.308),
        Mk1061 => Value::Real(real(&args[0]) * 0.154),
        Mk1062 => Value::Real(real(&args[0]) / 0.154),
        Mk1063 => Value::Real(real(&args[0]) * 0.05),
        Mk1064 => Value::Real(real(&args[0]) / 0.05),
        Mk1065 => Value::Real(real(&args[0]) * 59.0),
        Mk1066 => Value::Real(real(&args[0]) / 59.0),
        Mk1067 => Value::Real(real(&args[0]) * 177.0),
        Mk1068 => Value::Real(real(&args[0]) / 177.0),
        Mk1069 => Value::Real(1.618033988749895),
        Mk1070 => Value::Real(2.399963229728653),
        Mk1071 => Value::Real(2.414213562373095),
        Mk1072 => Value::Real(1.324717957244746),
        Mk1073 => Value::Real(1.465571231876768),
        Mk1074 => Value::Real(1.839286755214161),
        Mk1075 => Value::Real(2.718281828459045),
        Mk1076 => Value::Real(9.869604401089358),
        Mk1077 => Value::Real(6.283185307179586),
        Mk1078 => Value::Real(1.4142135623730951),
        Mk1079 => Value::Real(1.7320508075688772),
        Mk1080 => Value::Real(2.23606797749979),
        Mk1081 => Value::Real(2.6457513110645907),
        Mk1082 => Value::Real(1.2599210498948732),
        Mk1083 => Value::Real(0.6931471805599453),
        Mk1084 => Value::Real(2.302585092994046),
        Mk1085 => Value::Real(1.4426950408889634),
        Mk1086 => Value::Real(0.4342944819032518),
        Mk1087 => Value::Real(0.915965594177219),
        Mk1088 => Value::Real(1.2020569031595942),
        Mk1089 => Value::Real(0.5772156649015329),
        Mk1090 => Value::Real(4.66920160910299),
        Mk1091 => Value::Real(2.502907875095892),
        Mk1092 => Value::Real(1.303577269034296),
        Mk1093 => Value::Real(2.685452001065306),
        Mk1094 => Value::Real(1.282427129100623),
        Mk1095 => Value::Real(1.30637788386308),
        Mk1096 => Value::Real(0.834626841674073),
        Mk1097 => Value::Real(0.5671432904097838),
        Mk1098 => Value::Real(23.140692632779267),
        Mk1099 => Value::Real(2.665144142690225),
        Mk1100 => Value::Real(1.451369234883381),
        Mk1101 => Value::Real(1.606695152415291),
        Mk1102 => Value::Real(0.66016181584687),
        Mk1103 => Value::Real(1.902160583104),
        Mk1104 => Value::Real(0.62432998854355),
        Mk1105 => Value::Real(0.643410546288338),
        Mk1106 => Value::Real(0.662743419349182),
        Mk1107 => Value::Real(2.622057554292119),
        Mk1108 => Value::Real(2.094551481542327),
        Mk1109 => Value::Real(2.295587149392638),
        Mk1110 => Value::Real(2.584981759579253),
        Mk1111 => Value::Real(1.456074948582689),
        Mk1112 => Value::Real(1.467078079433975),
        Mk1113 => Value::Real(1.539600717839002),
        Mk1114 => Value::Real(1.705211140105368),
        Mk1115 => Value::Real(1.1319882487943),
        Mk1116 => Value::Real(1.187452351126501),
        Mk1117 => Value::Real(2.3484645415),
        Mk1118 => Value::Real(1.661687949633594),
        Mk1119 => Value::Real(299792458.0),
        Mk1120 => Value::Real(6.6743e-11),
        Mk1121 => Value::Real(6.62607015e-34),
        Mk1122 => Value::Real(1.054571817e-34),
        Mk1123 => Value::Real(1.380649e-23),
        Mk1124 => Value::Real(6.02214076e+23),
        Mk1125 => Value::Real(8.314462618),
        Mk1126 => Value::Real(1.602176634e-19),
        Mk1127 => Value::Real(9.1093837015e-31),
        Mk1128 => Value::Real(1.67262192369e-27),
        Mk1129 => Value::Real(1.67492749804e-27),
        Mk1130 => Value::Real(96485.33212),
        Mk1131 => Value::Real(5.670374419e-08),
        Mk1132 => Value::Real(8.8541878128e-12),
        Mk1133 => Value::Real(1.25663706212e-06),
        Mk1134 => Value::Real(0.0072973525693),
        Mk1135 => Value::Real(10973731.56816),
        Mk1136 => Value::Real(9.2740100783e-24),
        Mk1137 => Value::Real(5.0507837461e-27),
        Mk1138 => Value::Real(9.80665),
        Mk1139 => Value::Real(101325.0),
        Mk1140 => Value::Real(1.602176634e-19),
        Mk1141 => Value::Real(67.4),
        Mk1142 => Value::Real(1361.0),
        Mk1143 => Value::Real(9.80665),
        Mk1144 => Value::Real(343.0),
        Mk1145 => Value::Real(-273.15),
        Mk1146 => Value::Real(0.02271095464),
        Mk1147 => Value::Real(0.002897771955),
        Mk1148 => Value::Real(483597848400000.0),
        Mk1149 => Value::Real(25812.80745),

        // conversions
        ToInt => match &args[0] {
            Value::Str(s) => {
                let t = s.trim();
                Value::Int(t.parse().map_err(|_| format!("to_int: \"{}\" is not a whole number", s))?)
            }
            Value::Real(r) => to_int_checked(r.trunc())?,
            Value::Int(n) => Value::Int(*n),
            Value::Bool(v) => Value::Int(*v as i64),
            other => panic!("internal error: to_int of {:?}", other),
        },
        ToFloat => match &args[0] {
            Value::Str(s) => match s.trim().replace(',', ".").parse::<f64>() {
                Ok(r) if r.is_finite() => Value::Real(r),
                _ => return Err(format!("to_float: \"{}\" is not a number", s)),
            },
            v => Value::Real(real(v)),
        },
        ToText => string(display(&args[0])),
        Char => {
            let n = int(&args[0]);
            let c = u32::try_from(n)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("char: {} is not a valid character code", n))?;
            string(c.to_string())
        }
        Code => match text(&args[0]).chars().next() {
            Some(c) => Value::Int(c as i64),
            None => return Err("code: the string is empty".to_string()),
        },
        ToHex | ToBinary => {
            let n = int(&args[0]);
            let digits = if b == ToHex { format!("{:x}", n.unsigned_abs()) } else { format!("{:b}", n.unsigned_abs()) };
            string(if n < 0 { format!("-{}", digits) } else { digits })
        }

        // math
        Absolute => match &args[0] {
            Value::Int(n) => Value::Int(n.checked_abs().ok_or(OVERFLOW)?),
            v => Value::Real(real(v).abs()),
        },
        Minimum | Maximum => {
            let pick = |a: Value, c: Value| {
                let take_c = match compare(&c, &a) {
                    Ordering::Less => b == Minimum,
                    Ordering::Greater => b == Maximum,
                    Ordering::Equal => false,
                };
                if take_c { c } else { a }
            };
            if args.len() == 2 {
                pick(args[0].clone(), args[1].clone())
            } else {
                let items = seq(&args[0]).borrow();
                no_empty_elements(&items, b.name())?;
                let mut it = items.iter().cloned();
                let first = it.next().ok_or_else(|| format!("{}: the list is empty", b.name()))?;
                it.fold(first, pick)
            }
        }
        Power => Value::Real(real(&args[0]).powf(real(&args[1]))),
        Floor => to_int_checked(real(&args[0]).floor())?,
        Ceiling => to_int_checked(real(&args[0]).ceil())?,
        Truncate => to_int_checked(real(&args[0]).trunc())?,
        RoundTo => {
            let digits = int(&args[1]).clamp(-15, 15) as i32;
            let scale = 10f64.powi(digits);
            Value::Real((real(&args[0]) * scale).round() / scale)
        }
        Exponential => Value::Real(real(&args[0]).exp()),
        Logarithm => {
            let x = real(&args[0]);
            if x <= 0.0 {
                return Err(format!("logarithm of a number that is not positive ({})", format_real(x)));
            }
            match args.get(1).map(real) {
                None => Value::Real(x.ln()),
                Some(base) if base > 0.0 && base != 1.0 => Value::Real(x.ln() / base.ln()),
                Some(base) => return Err(format!("logarithm: the base must be positive and not 1, here it is {}", format_real(base))),
            }
        }
        Tan => Value::Real(real(&args[0]).to_radians().tan()),
        Arcsin | Arccos => {
            let x = real(&args[0]);
            if !(-1.0..=1.0).contains(&x) {
                return Err(format!("{} works with numbers from -1 to 1, here it is {}", b.name(), format_real(x)));
            }
            Value::Real(degrees(if b == Arcsin { x.asin() } else { x.acos() }))
        }
        Arctan => Value::Real(degrees(real(&args[0]).atan())),
        Arctan2 => Value::Real(degrees(real(&args[0]).atan2(real(&args[1])))),
        Pi => Value::Real(std::f64::consts::PI),

        // strings
        Uppercase => string(text(&args[0]).to_uppercase()),
        Lowercase => string(text(&args[0]).to_lowercase()),
        Trim => string(text(&args[0]).trim()),
        Find => Value::Int(find(text(&args[0]), text(&args[1]))),
        Count => {
            let part = text(&args[1]);
            if part.is_empty() {
                return Err("count: the part to look for is empty".to_string());
            }
            Value::Int(text(&args[0]).matches(part).count() as i64)
        }
        Replace => {
            let old = text(&args[1]);
            if old.is_empty() {
                return Err("replace: the text to replace is empty".to_string());
            }
            string(text(&args[0]).replace(old, text(&args[2])))
        }
        Split => {
            let (s, sep) = (text(&args[0]), text(&args[1]));
            let pieces: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| string(c.to_string())).collect()
            } else {
                s.split(sep).map(string).collect()
            };
            list(pieces)
        }
        Join => {
            let items = seq(&args[0]).borrow();
            no_empty_elements(&items, "join")?;
            let parts: Vec<String> = items.iter().map(display).collect();
            string(parts.join(text(&args[1])))
        }
        StartsWith => Value::Bool(text(&args[0]).starts_with(text(&args[1]))),
        EndsWith => Value::Bool(text(&args[0]).ends_with(text(&args[1]))),
        Substring => {
            let chars: Vec<char> = text(&args[0]).chars().collect();
            let from = int(&args[1]).max(1);
            let to = int(&args[2]).min(chars.len() as i64);
            if from > to {
                string("")
            } else {
                string(chars[from as usize - 1..to as usize].iter().collect::<String>())
            }
        }
        Add => {
            set_data(&args[0]).borrow_mut().set(DictKey::of(&args[1]), Value::Bool(true));
            Value::Null
        }
        Discard => {
            set_data(&args[0]).borrow_mut().remove(&DictKey::of(&args[1]));
            Value::Null
        }
        ToList => new_list(set_data(&args[0]).borrow().keys().map(DictKey::value).collect()),
        Contains | IndexOf => match &args[0] {
            Value::Set(d) => Value::Bool(d.borrow().get(&DictKey::of(&args[1])).is_some()),
            Value::Str(s) => Value::Bool(s.contains(text(&args[1]))),
            c => {
                let items = seq(c).borrow();
                let at = items.iter().position(|v| !matches!(v, Value::Null) && same(v, &args[1]));
                if b == Contains {
                    Value::Bool(at.is_some())
                } else {
                    Value::Int(at.map_or(0, |i| i as i64 + 1))
                }
            }
        },

        // collections
        Sum => {
            let items = seq(&args[0]).borrow();
            no_empty_elements(&items, "sum")?;
            match items.first() {
                Some(Value::Real(_)) => Value::Real(items.iter().map(real).sum()),
                _ => {
                    let mut total: i64 = 0;
                    for v in items.iter() {
                        total = total.checked_add(int(v)).ok_or(OVERFLOW)?;
                    }
                    Value::Int(total)
                }
            }
        }
        Sort => {
            let mut items = seq(&args[0]).borrow_mut();
            no_empty_elements(&items, "sort")?;
            items.sort_by(compare);
            Value::Null
        }
        Reverse => {
            seq(&args[0]).borrow_mut().reverse();
            Value::Null
        }

        // dictionaries
        HasKey => Value::Bool(dict(&args[0]).borrow().get(&DictKey::of(&args[1])).is_some()),
        Get => dict(&args[0]).borrow().get(&DictKey::of(&args[1])).cloned().unwrap_or_else(|| args[2].clone()),
        RemoveKey => {
            let k = DictKey::of(&args[1]);
            if !dict(&args[0]).borrow_mut().remove(&k) {
                return Err(format!("remove_key: {}", missing_key(&k)));
            }
            Value::Null
        }
        Keys => new_list(dict(&args[0]).borrow().keys().map(DictKey::value).collect()),
        Values => new_list(dict(&args[0]).borrow().values().cloned().collect()),
        Insert => {
            let mut items = seq(&args[0]).borrow_mut();
            let pos = int(&args[1]);
            if pos < 1 || pos as usize > items.len() + 1 {
                return Err(format!("insert: the position must be from 1 to {}, here it is {}", items.len() + 1, pos));
            }
            items.insert(pos as usize - 1, args[2].clone());
            Value::Null
        }
        TakeLast => seq(&args[0]).borrow_mut().pop().ok_or("take_last: the list is empty")?,
        ClearList => {
            seq(&args[0]).borrow_mut().clear();
            Value::Null
        }

        // files
        ReadFile => {
            let path = text(&args[0]);
            string(std::fs::read_to_string(path).map_err(|e| format!("cannot read the file \"{}\": {}", path, e))?)
        }
        ReadLines => {
            let path = text(&args[0]);
            let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read the file \"{}\": {}", path, e))?;
            list(content.lines().map(string).collect())
        }
        WriteFile => {
            let path = text(&args[0]);
            std::fs::write(path, text(&args[1])).map_err(|e| format!("cannot write the file \"{}\": {}", path, e))?;
            Value::Null
        }
        AppendFile => {
            use std::io::Write;
            let path = text(&args[0]);
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| format!("cannot write the file \"{}\": {}", path, e))?;
            f.write_all(text(&args[1]).as_bytes()).map_err(|e| format!("cannot write the file \"{}\": {}", path, e))?;
            Value::Null
        }
        FileExists => Value::Bool(std::path::Path::new(text(&args[0])).exists()),

        // system
        Wait => {
            let ms = int(&args[0]).clamp(0, 86_400_000) as u64;
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Value::Null
        }
        Today => {
            let t = local_time();
            string(format!("{:04}-{:02}-{:02}", t.0, t.1, t.2))
        }
        Now => {
            let t = local_time();
            string(format!("{:02}:{:02}:{:02}", t.3, t.4, t.5))
        }
        TextMatches => Value::Bool(pattern(&args[1])?.is_match(text(&args[0]))),
        TextFind => string(pattern(&args[1])?.find(text(&args[0])).map_or("", |m| m.as_str())),
        TextReplaceAll => string(pattern(&args[1])?.replace_all(text(&args[0]), text(&args[2])).into_owned()),
        TextAll => list(pattern(&args[1])?.find_iter(text(&args[0])).map(|m| string(m.as_str())).collect()),
        TextSplit => list(pattern(&args[1])?.split(text(&args[0])).map(string).collect()),
        TextCount => Value::Int(pattern(&args[1])?.find_iter(text(&args[0])).count() as i64),
        TextGroup => {
            let n = int(&args[2]);
            let re = pattern(&args[1])?;
            let g = if n < 0 {
                ""
            } else {
                match re.captures(text(&args[0])) {
                    Some(c) => c.get(n as usize).map_or("", |m| m.as_str()),
                    None => "",
                }
            };
            string(g)
        }
        ChartLine => chart_write(&args[0], None, ChartKind::Line, text(&args[1]))?,
        ChartBars => chart_write(&args[0], None, ChartKind::Bars, text(&args[1]))?,
        ChartScatter => chart_write(&args[0], Some(&args[1]), ChartKind::Scatter, text(&args[2]))?,

        // module "table"
        TableHeaders => {
            let lines = csv_lines(text(&args[0]));
            let header = lines.first().ok_or("table: the CSV is empty")?;
            list(csv_fields(header).into_iter().map(string).collect())
        }
        TableRows => Value::Int(csv_lines(text(&args[0])).len().saturating_sub(1) as i64),
        TableColumn => {
            let (lines, idx) = table_col_index(text(&args[0]), text(&args[1]))?;
            list(lines.iter().skip(1).map(|l| string(csv_fields(l).get(idx).cloned().unwrap_or_default())).collect())
        }
        TableNumbers => {
            let (lines, idx) = table_col_index(text(&args[0]), text(&args[1]))?;
            let mut out = Vec::new();
            for l in lines.iter().skip(1) {
                out.push(Value::Real(table_cell_number(&csv_fields(l), idx)?));
            }
            list(out)
        }
        TableMean | TableSum => {
            let (lines, idx) = table_col_index(text(&args[0]), text(&args[1]))?;
            let (mut sum, mut count) = (0.0, 0usize);
            for l in lines.iter().skip(1) {
                sum += table_cell_number(&csv_fields(l), idx)?;
                count += 1;
            }
            if b == TableMean {
                if count == 0 {
                    return Err("table.mean: the column has no values".to_string());
                }
                Value::Real(sum / count as f64)
            } else {
                Value::Real(sum)
            }
        }

        // module "iter"
        IterCombinations => list(char_combinations(text(&args[0]), int(&args[1]))?.into_iter().map(string).collect()),
        IterPermutations => list(char_permutations(text(&args[0]), int(&args[1]))?.into_iter().map(string).collect()),
        IterProduct => list(char_product(text(&args[0]), text(&args[1])).into_iter().map(string).collect()),

        // module "pick" (random over lists)
        PickShuffle => {
            let mut items = seq(&args[0]).borrow_mut();
            let n = items.len();
            for i in (1..n).rev() {
                let j = (rng_u64() % (i as u64 + 1)) as usize;
                items.swap(i, j);
            }
            Value::Null
        }
        PickChoice => {
            let items = seq(&args[0]).borrow();
            if items.is_empty() {
                return Err("pick.choice: the list is empty".to_string());
            }
            items[(rng_u64() % items.len() as u64) as usize].clone()
        }
        PickSample => {
            let k = int(&args[1]);
            let items = seq(&args[0]).borrow();
            if k < 0 || k as usize > items.len() {
                return Err(format!("pick.sample: cannot take {} from a list of {}", k, items.len()));
            }
            let mut pool: Vec<Value> = items.clone();
            let n = pool.len();
            for i in 0..k as usize {
                let j = i + (rng_u64() % (n - i) as u64) as usize;
                pool.swap(i, j);
            }
            pool.truncate(k as usize);
            new_list(pool)
        }

        // module "clock" (timestamp arithmetic)
        ClockAdd => Value::Int(int(&args[0]).checked_add(int(&args[1])).ok_or(OVERFLOW)?),
        ClockDiff => Value::Int(int(&args[0]).checked_sub(int(&args[1])).ok_or(OVERFLOW)?),

        // ======== batch G1 ========
        AccelGToMps2 => Value::Real(real(&args[0]) * 9.806_65),
        AccelMps2ToG => Value::Real(real(&args[0]) / 9.806_65),
        LuxFcToLux => Value::Real(real(&args[0]) * 10.763_910),
        LuxLuxToFc => Value::Real(real(&args[0]) / 10.763_910),
        RadGrayToRad => Value::Real(real(&args[0]) * 100.0),
        RadRadToGray => Value::Real(real(&args[0]) / 100.0),
        RadSvToRem => Value::Real(real(&args[0]) * 100.0),
        RadRemToSv => Value::Real(real(&args[0]) / 100.0),
        TurnToDeg => Value::Real(real(&args[0]) * 360.0),
        TurnFromDeg => Value::Real(real(&args[0]) / 360.0),
        TurnToRad => Value::Real(real(&args[0]) * std::f64::consts::TAU),
        TurnFromRad => Value::Real(real(&args[0]) / std::f64::consts::TAU),
        FlowLpmToM3h => Value::Real(real(&args[0]) * 0.06),
        FlowM3hToLpm => Value::Real(real(&args[0]) / 0.06),
        TypoPtToMm => Value::Real(real(&args[0]) * 0.352_777_778),
        TypoMmToPt => Value::Real(real(&args[0]) / 0.352_777_778),
        TypoPtToInch => Value::Real(real(&args[0]) / 72.0),
        TypoInchToPt => Value::Real(real(&args[0]) * 72.0),
        OvenCToGas => Value::Real((real(&args[0]) - 121.0) / 14.0),
        OvenGasToC => Value::Real(real(&args[0]) * 14.0 + 121.0),
        GlucMgdlToMmol => Value::Real(real(&args[0]) / 18.0182),
        GlucMmolToMgdl => Value::Real(real(&args[0]) * 18.0182),
        ScreenPpi => {
            let d = real(&args[2]);
            if d == 0.0 {
                return Err("screen.ppi: the diagonal is zero".to_string());
            }
            Value::Real(real(&args[0]).hypot(real(&args[1])) / d)
        }
        AspectRatio => {
            let h = real(&args[1]);
            if h == 0.0 {
                return Err("aspect.ratio: the height is zero".to_string());
            }
            Value::Real(real(&args[0]) / h)
        }
        BeaufortFromKmh => Value::Int(beaufort(real(&args[0]))),
        QuakeEnergy => Value::Real(10f64.powf(1.5 * real(&args[0]) + 4.8)),
        WrapvInto => {
            let (x, lo, hi) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(if (hi - lo).abs() < 1e-12 { lo } else { lo + (x - lo).rem_euclid(hi - lo) })
        }
        SmoothstepOf => {
            let (e0, e1, x) = (real(&args[0]), real(&args[1]), real(&args[2]));
            let t = if e1 == e0 { 0.0 } else { ((x - e0) / (e1 - e0)).clamp(0.0, 1.0) };
            Value::Real(t * t * (3.0 - 2.0 * t))
        }
        StepAt => Value::Real(if real(&args[1]) >= real(&args[0]) { 1.0 } else { 0.0 }),
        SincOf => {
            let x = real(&args[0]);
            Value::Real(if x == 0.0 { 1.0 } else { x.sin() / x })
        }
        SigmoidOf => Value::Real(1.0 / (1.0 + (-real(&args[0])).exp())),
        ReluOf => Value::Real(real(&args[0]).max(0.0)),
        SoftsignOf => {
            let x = real(&args[0]);
            Value::Real(x / (1.0 + x.abs()))
        }
        GaussPdf => {
            let (x, m, s) = (real(&args[0]), real(&args[1]), real(&args[2]));
            if s == 0.0 {
                return Err("gauss.pdf: the deviation is zero".to_string());
            }
            let z = (x - m) / s;
            Value::Real((-0.5 * z * z).exp() / (s * std::f64::consts::TAU.sqrt()))
        }
        ErfOf => Value::Real(erf(real(&args[0]))),
        GammafOf => Value::Real(gamma(real(&args[0]))),
        HarmonicNumber => {
            let n = int(&args[0]);
            if n < 0 {
                return Err("harmonic.number: n cannot be negative".to_string());
            }
            Value::Real((1..=n).map(|k| 1.0 / k as f64).sum())
        }
        GeoseriesSum => {
            let (a, r, n) = (real(&args[0]), real(&args[1]), int(&args[2]));
            if n < 0 {
                return Err("geoseries.sum: n cannot be negative".to_string());
            }
            Value::Real(if (r - 1.0).abs() < 1e-12 { a * n as f64 } else { a * (1.0 - r.powi(n as i32)) / (1.0 - r) })
        }
        ArithseriesSum => {
            let (a, d, n) = (real(&args[0]), real(&args[1]), int(&args[2]));
            if n < 0 {
                return Err("arithseries.sum: n cannot be negative".to_string());
            }
            let nf = n as f64;
            Value::Real(nf / 2.0 * (2.0 * a + (nf - 1.0) * d))
        }
        ModuloWrap => {
            let n = int(&args[1]);
            if n == 0 {
                return Err("modulo.wrap: the modulus is zero".to_string());
            }
            Value::Int(int(&args[0]).rem_euclid(n))
        }
        ModuloFloordiv => {
            let n = int(&args[1]);
            if n == 0 {
                return Err("modulo.floordiv: division by zero".to_string());
            }
            Value::Int(int(&args[0]).div_euclid(n))
        }
        Hypot3Of => {
            let (x, y, z) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real((x * x + y * y + z * z).sqrt())
        }
        ClampfBetween => Value::Real(real(&args[0]).max(real(&args[1])).min(real(&args[2]))),
        FracPart => Value::Real(real(&args[0]).fract()),
        FracWhole => Value::Real(real(&args[0]).trunc()),
        ReciprocalOf => {
            let x = real(&args[0]);
            if x == 0.0 {
                return Err("reciprocal.of: the number is zero".to_string());
            }
            Value::Real(1.0 / x)
        }
        PreciseLog1p => Value::Real(real(&args[0]).ln_1p()),
        PreciseExpm1 => Value::Real(real(&args[0]).exp_m1()),
        RotstrLeft => string(rotate_str(text(&args[0]), int(&args[1]))),
        RotstrRight => string(rotate_str(text(&args[0]), -int(&args[1]))),
        ZfillOf => string(pad(text(&args[0]), int(&args[1]), "0", true)),
        NospacesOf => string(text(&args[0]).chars().filter(|c| !c.is_whitespace()).collect::<String>()),
        DigitsonlyOf => string(text(&args[0]).chars().filter(|c| c.is_ascii_digit()).collect::<String>()),
        LettersonlyOf => string(text(&args[0]).chars().filter(|c| c.is_alphabetic()).collect::<String>()),
        DisemvowelOf => string(text(&args[0]).chars().filter(|c| !"aeiouAEIOU".contains(*c)).collect::<String>()),
        ScreamingOf => string(case_join(text(&args[0]), '_').to_uppercase()),
        DotcaseOf => string(case_join(text(&args[0]), '.')),
        PathcaseOf => string(case_join(text(&args[0]), '/')),
        SentencecaseOf => string(sentence_case(text(&args[0]))),
        DedupChars => string(dedup_chars(text(&args[0]))),
        ExpandtabsOf => string(text(&args[0]).replace('\t', &" ".repeat(int(&args[1]).max(0) as usize))),
        Rot5Digits => string(
            text(&args[0]).chars().map(|c| if c.is_ascii_digit() { (b'0' + ((c as u8 - b'0') + 5) % 10) as char } else { c }).collect::<String>(),
        ),
        HammingDistance => Value::Int(hamming(text(&args[0]), text(&args[1]))?),
        LevenshteinDistance => Value::Int(levenshtein(text(&args[0]), text(&args[1]))),
        SoundexOf => string(soundex(text(&args[0]))),
        SyllablesCount => Value::Int(syllables(text(&args[0]))),
        AvgwordAverage => {
            let ws: Vec<&str> = text(&args[0]).split_whitespace().collect();
            if ws.is_empty() {
                Value::Real(0.0)
            } else {
                Value::Real(ws.iter().map(|w| w.chars().count()).sum::<usize>() as f64 / ws.len() as f64)
            }
        }
        ReadtimeMinutes => Value::Real(text(&args[0]).split_whitespace().count() as f64 / 200.0),

        // ======== batch G2 ========
        KinVfinal => Value::Real(real(&args[0]) + real(&args[1]) * real(&args[2])),
        KinDistance => {
            let (v0, a, t) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(v0 * t + 0.5 * a * t * t)
        }
        ProjRange => Value::Real(real(&args[0]).powi(2) * (2.0 * real(&args[1]).to_radians()).sin() / 9.80665),
        ProjMaxHeight => {
            Value::Real(real(&args[0]).powi(2) * real(&args[1]).to_radians().sin().powi(2) / (2.0 * 9.80665))
        }
        ProjFlightTime => Value::Real(2.0 * real(&args[0]) * real(&args[1]).to_radians().sin() / 9.80665),
        PendPeriod => Value::Real(std::f64::consts::TAU * (real(&args[0]) / 9.80665).sqrt()),
        SpringPeriod => {
            let k = real(&args[1]);
            if k <= 0.0 {
                return Err("spring.period: the stiffness must be positive".to_string());
            }
            Value::Real(std::f64::consts::TAU * (real(&args[0]) / k).sqrt())
        }
        SpringForce => Value::Real(real(&args[0]) * real(&args[1])),
        LensFocal => {
            let s = 1.0 / real(&args[0]) + 1.0 / real(&args[1]);
            if s == 0.0 {
                return Err("lens.focal: these distances have no focus".to_string());
            }
            Value::Real(1.0 / s)
        }
        LensDiopter => {
            let f = real(&args[0]);
            if f == 0.0 {
                return Err("lens.diopter: the focal length is zero".to_string());
            }
            Value::Real(1.0 / f)
        }
        ResSeries | CapParallel => Value::Real(real(&args[0]) + real(&args[1])),
        ResParallel | CapSeries => {
            let (a, b2) = (real(&args[0]), real(&args[1]));
            if a + b2 == 0.0 {
                return Err("the two values sum to zero".to_string());
            }
            Value::Real(a * b2 / (a + b2))
        }
        BuoyForce => Value::Real(real(&args[0]) * real(&args[1]) * 9.80665),
        SpecGravity => Value::Real(real(&args[0]) / 1000.0),
        ThermoEnergy => Value::Real(real(&args[0]) * real(&args[1]) * real(&args[2])),
        EfficiencyPercent => {
            let i = real(&args[1]);
            if i == 0.0 {
                return Err("efficiency.percent: the input is zero".to_string());
            }
            Value::Real(real(&args[0]) / i * 100.0)
        }
        WorkJoules | MomentOf | ImpulseOf => Value::Real(real(&args[0]) * real(&args[1])),
        DopplerObserved => {
            let (f, vs, vsound) = (real(&args[0]), real(&args[1]), real(&args[2]));
            if vsound - vs == 0.0 {
                return Err("doppler.observed: the source travels at the speed of sound".to_string());
            }
            Value::Real(f * vsound / (vsound - vs))
        }
        EscapeVelocity => {
            let r = real(&args[1]);
            if r <= 0.0 {
                return Err("escape.velocity: the radius must be positive".to_string());
            }
            Value::Real((2.0 * 6.674e-11 * real(&args[0]) / r).sqrt())
        }
        OrbitPeriod => {
            let m = real(&args[1]);
            if m <= 0.0 {
                return Err("orbit.period: the mass must be positive".to_string());
            }
            Value::Real(std::f64::consts::TAU * (real(&args[0]).powi(3) / (6.674e-11 * m)).sqrt())
        }
        RedshiftZ => {
            let rest = real(&args[1]);
            if rest == 0.0 {
                return Err("redshift.z: the rest wavelength is zero".to_string());
            }
            Value::Real((real(&args[0]) - rest) / rest)
        }
        LorentzFactor => {
            let b2 = real(&args[0]) / 299_792_458.0;
            if b2.abs() >= 1.0 {
                return Err("lorentz.factor: the speed must be below the speed of light".to_string());
            }
            Value::Real(1.0 / (1.0 - b2 * b2).sqrt())
        }
        HalflifeRemaining => {
            let hl = real(&args[2]);
            if hl == 0.0 {
                return Err("halflife.remaining: the half-life is zero".to_string());
            }
            Value::Real(real(&args[0]) * 0.5f64.powf(real(&args[1]) / hl))
        }
        DecayAmount => Value::Real(real(&args[0]) * (-real(&args[1]) * real(&args[2])).exp()),
        GrowthAmount => Value::Real(real(&args[0]) * (real(&args[1]) * real(&args[2])).exp()),
        Rule72Years => {
            let r = real(&args[0]);
            if r == 0.0 {
                return Err("rule72.years: the rate is zero".to_string());
            }
            Value::Real(72.0 / r)
        }
        AnnuityPv => {
            let (pmt, rate, n) = (real(&args[0]), real(&args[1]), int(&args[2]));
            if n < 0 {
                return Err("annuity.present_value: the periods cannot be negative".to_string());
            }
            let r = rate / 100.0;
            Value::Real(if r == 0.0 { pmt * n as f64 } else { pmt * (1.0 - (1.0 + r).powi(-(n as i32))) / r })
        }
        BreakevenUnits => {
            let (fixed, price, var) = (real(&args[0]), real(&args[1]), real(&args[2]));
            if price - var == 0.0 {
                return Err("breakeven.units: the price equals the variable cost".to_string());
            }
            Value::Real(fixed / (price - var))
        }
        ProfitAmount => Value::Real(real(&args[0]) - real(&args[1])),
        ProfitMargin => {
            let rev = real(&args[0]);
            if rev == 0.0 {
                return Err("profit.margin: the revenue is zero".to_string());
            }
            Value::Real((rev - real(&args[1])) / rev * 100.0)
        }
        CommissionOf => Value::Real(real(&args[0]) * real(&args[1]) / 100.0),
        WageWeekly => Value::Real(real(&args[0]) * real(&args[1])),
        WageOvertime => {
            let (hr, hours) = (real(&args[0]), real(&args[1]));
            Value::Real(hours.min(40.0) * hr + (hours - 40.0).max(0.0) * hr * 1.5)
        }
        InflationAdjust => Value::Real(real(&args[0]) * (1.0 + real(&args[1]) / 100.0).powf(real(&args[2]))),
        IdealweightForHeight => Value::Real(50.0 + 0.9 * (real(&args[0]) - 152.0)),
        TdeeFromBmr => Value::Real(real(&args[0]) * real(&args[1])),
        WaterDaily => Value::Real(real(&args[0]) * 35.0),
        DosageByWeight => Value::Real(real(&args[0]) * real(&args[1])),
        GforceTurn => {
            let r = real(&args[1]);
            if r <= 0.0 {
                return Err("gforce.turn: the radius must be positive".to_string());
            }
            Value::Real(real(&args[0]).powi(2) / (r * 9.80665))
        }
        GearRatio => {
            let d = real(&args[1]);
            if d == 0.0 {
                return Err("gear.ratio: the driver has zero teeth".to_string());
            }
            Value::Real(real(&args[0]) / d)
        }
        AutonomyDistance | EmissionsFromKm => Value::Real(real(&args[0]) * real(&args[1])),
        SolarWatts => Value::Real(real(&args[0]) * real(&args[1]) * real(&args[2]) / 100.0),
        BatteryRuntime => {
            let d = real(&args[1]);
            if d == 0.0 {
                return Err("battery.runtime: the current draw is zero".to_string());
            }
            Value::Real(real(&args[0]) / d)
        }
        LedResistor => {
            let c = real(&args[2]);
            if c == 0.0 {
                return Err("led.resistor: the current is zero".to_string());
            }
            Value::Real((real(&args[0]) - real(&args[1])) / (c / 1000.0))
        }
        FreefallVelocity => Value::Real(9.80665 * real(&args[0])),
        FreefallDistance => {
            let t = real(&args[0]);
            Value::Real(0.5 * 9.80665 * t * t)
        }
        TetraVolume => {
            let e = real(&args[0]);
            Value::Real(e * e * e / (6.0 * 2f64.sqrt()))
        }
        TetraSurface => {
            let e = real(&args[0]);
            Value::Real(3f64.sqrt() * e * e)
        }
        OctaVolume => {
            let e = real(&args[0]);
            Value::Real(2f64.sqrt() / 3.0 * e * e * e)
        }
        ParallelogramArea => Value::Real(real(&args[0]) * real(&args[1])),
        RhombusArea => Value::Real(real(&args[0]) * real(&args[1]) / 2.0),
        SectorArea => Value::Real(std::f64::consts::PI * real(&args[0]).powi(2) * real(&args[1]) / 360.0),
        SectorArc => Value::Real(std::f64::consts::TAU * real(&args[0]) * real(&args[1]) / 360.0),
        EllipsoidVolume => {
            Value::Real(4.0 / 3.0 * std::f64::consts::PI * real(&args[0]) * real(&args[1]) * real(&args[2]))
        }
        NgonPerimeter => {
            let n = int(&args[0]);
            if n < 3 {
                return Err("ngon.perimeter: a polygon needs at least 3 sides".to_string());
            }
            Value::Real(n as f64 * real(&args[1]))
        }
        NgonApothem => {
            let n = int(&args[0]);
            if n < 3 {
                return Err("ngon.apothem: a polygon needs at least 3 sides".to_string());
            }
            Value::Real(real(&args[1]) / (2.0 * (std::f64::consts::PI / n as f64).tan()))
        }
        FrustumVolume => {
            let (r1, r2, h) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(std::f64::consts::PI * h * (r1 * r1 + r1 * r2 + r2 * r2) / 3.0)
        }
        InclinePctToDeg => Value::Real((real(&args[0]) / 100.0).atan().to_degrees()),
        InclineDegToPct => Value::Real(real(&args[0]).to_radians().tan() * 100.0),

        // ======== batch G3 ========
        ApproxEqual => Value::Bool((real(&args[0]) - real(&args[1])).abs() <= real(&args[2])),
        ProportionSolve => {
            let a = real(&args[0]);
            if a == 0.0 {
                return Err("proportion.solve: the first term is zero".to_string());
            }
            Value::Real(real(&args[1]) * real(&args[2]) / a)
        }
        MeansOf2 => Value::Real((real(&args[0]) + real(&args[1])) / 2.0),
        MeansGeometric2 => {
            let (a, b2) = (real(&args[0]), real(&args[1]));
            if a < 0.0 || b2 < 0.0 {
                return Err("means.geometric2: the values must be non-negative".to_string());
            }
            Value::Real((a * b2).sqrt())
        }
        MeansHarmonic2 => {
            let (a, b2) = (real(&args[0]), real(&args[1]));
            if a + b2 == 0.0 {
                return Err("means.harmonic2: the values sum to zero".to_string());
            }
            Value::Real(2.0 * a * b2 / (a + b2))
        }
        BankersRound => Value::Int(real(&args[0]).round_ties_even() as i64),
        PluralOf => {
            let (w, n) = (text(&args[0]), int(&args[1]));
            string(if n == 1 { w.to_string() } else { format!("{}s", w) })
        }
        AbbreviateNumber => string(abbreviate_number(real(&args[0]))),
        GpaFromPercent => {
            let p = real(&args[0]);
            Value::Real(if p >= 90.0 { 4.0 } else if p >= 80.0 { 3.0 } else if p >= 70.0 { 2.0 } else if p >= 60.0 { 1.0 } else { 0.0 })
        }
        CurveGrade => {
            let m = real(&args[1]);
            if m == 0.0 {
                return Err("curve.grade: the maximum is zero".to_string());
            }
            Value::Real(real(&args[0]) / m * 100.0)
        }
        ZscoreOf => {
            let s = real(&args[2]);
            if s == 0.0 {
                return Err("zscore.of: the deviation is zero".to_string());
            }
            Value::Real((real(&args[0]) - real(&args[1])) / s)
        }
        MinmaxnormOf => {
            let (x, lo, hi) = (real(&args[0]), real(&args[1]), real(&args[2]));
            if hi == lo {
                return Err("minmaxnorm.of: the range is empty".to_string());
            }
            Value::Real((x - lo) / (hi - lo))
        }
        Clamp01Of => Value::Real(real(&args[0]).clamp(0.0, 1.0)),
        CerpOf => {
            let (a, b2, t) = (real(&args[0]), real(&args[1]), real(&args[2]));
            let f = (1.0 - (t * std::f64::consts::PI).cos()) / 2.0;
            Value::Real(a * (1.0 - f) + b2 * f)
        }
        NibbleHigh => Value::Int((int(&args[0]) >> 4) & 0xF),
        NibbleLow => Value::Int(int(&args[0]) & 0xF),
        ByteopsHigh => Value::Int((int(&args[0]) >> 8) & 0xFF),
        ByteopsLow => Value::Int(int(&args[0]) & 0xFF),
        EndianSwap16 => {
            let n = int(&args[0]) & 0xFFFF;
            Value::Int(((n & 0xFF) << 8) | ((n >> 8) & 0xFF))
        }
        EndianSwap32 => Value::Int((int(&args[0]) as u32).swap_bytes() as i64),
        Nextpow2Of => {
            let n = int(&args[0]);
            Value::Int(if n < 1 { 1 } else { (n as u64).next_power_of_two() as i64 })
        }
        Ispow2Of => {
            let n = int(&args[0]);
            Value::Bool(n > 0 && (n & (n - 1)) == 0)
        }
        AlignUp => {
            let a = int(&args[1]);
            if a <= 0 {
                return Err("align.up: the alignment must be positive".to_string());
            }
            Value::Int((int(&args[0]) + a - 1).div_euclid(a) * a)
        }
        AlignDown => {
            let a = int(&args[1]);
            if a <= 0 {
                return Err("align.down: the alignment must be positive".to_string());
            }
            Value::Int(int(&args[0]).div_euclid(a) * a)
        }
        GraycodeEncode => {
            let n = int(&args[0]);
            Value::Int(n ^ (n >> 1))
        }
        GraycodeDecode => Value::Int(gray_decode(int(&args[0]))),
        EanCheckDigit => Value::Int(ean_check(text(&args[0]))?),
        CardtypeOf => string(cardtype(text(&args[0]))),
        HumidexOf => {
            let (t, dp) = (real(&args[0]), real(&args[1]));
            let e = 6.11 * (5417.7530 * (1.0 / 273.16 - 1.0 / (dp + 273.15))).exp();
            Value::Real(t + 0.5555 * (e - 10.0))
        }
        PhFromConcentration => {
            let h = real(&args[0]);
            if h <= 0.0 {
                return Err("ph.from_concentration: the concentration must be positive".to_string());
            }
            Value::Real(-h.log10())
        }
        PhConcentration => Value::Real(10f64.powf(-real(&args[0]))),
        SoundAdd => {
            let (d1, d2) = (real(&args[0]), real(&args[1]));
            Value::Real(10.0 * (10f64.powf(d1 / 10.0) + 10f64.powf(d2 / 10.0)).log10())
        }
        NoteFrequency => Value::Real(440.0 * 2f64.powf(real(&args[0]) / 12.0)),
        NoteCents => {
            let (f1, f2) = (real(&args[0]), real(&args[1]));
            if f1 <= 0.0 || f2 <= 0.0 {
                return Err("note.cents: the frequencies must be positive".to_string());
            }
            Value::Real(1200.0 * (f2 / f1).log2())
        }
        ProbAnd => Value::Real(real(&args[0]) * real(&args[1])),
        ProbOr => {
            let (a, b2) = (real(&args[0]), real(&args[1]));
            Value::Real(a + b2 - a * b2)
        }
        ProbInvert => Value::Real(1.0 - real(&args[0])),
        OddsToProb => {
            let o = real(&args[0]);
            Value::Real(o / (1.0 + o))
        }
        OddsFromProb => {
            let p = real(&args[0]);
            if p >= 1.0 {
                return Err("odds.from_prob: the probability must be below 1".to_string());
            }
            Value::Real(p / (1.0 - p))
        }
        CollatzmaxPeak => Value::Int(collatz_peak(int(&args[0]))?),
        HappyIs => Value::Bool(happy_is(int(&args[0]))),
        AliquotIsAbundant => {
            let n = int(&args[0]);
            Value::Bool(n >= 1 && (divisor_sum(n)? - n) > n)
        }
        AliquotIsDeficient => {
            let n = int(&args[0]);
            Value::Bool(n >= 1 && (divisor_sum(n)? - n) < n)
        }
        AliquotIsPerfect => {
            let n = int(&args[0]);
            Value::Bool(n >= 1 && (divisor_sum(n)? - n) == n)
        }
        PolygonalNumber => {
            let (s, n) = (int(&args[0]), int(&args[1]));
            if s < 3 {
                return Err("polygonal.number: a polygon needs at least 3 sides".to_string());
            }
            Value::Int(((s - 2) * n * n - (s - 4) * n) / 2)
        }
        PronicNumber => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n + 1).ok_or(OVERFLOW)?)
        }
        PentagonalNumber => {
            let n = int(&args[0]);
            Value::Int(n * (3 * n - 1) / 2)
        }
        HexagonalNumber => {
            let n = int(&args[0]);
            Value::Int(n * (2 * n - 1))
        }
        StarNumber => {
            let n = int(&args[0]);
            Value::Int(6 * n * (n - 1) + 1)
        }
        LucasNumber => Value::Int(lucas(int(&args[0]))?),
        TribonacciNumber => Value::Int(tribonacci(int(&args[0]))?),
        PadovanNumber => Value::Int(padovan(int(&args[0]))?),
        PrimorialOf => Value::Int(primorial(int(&args[0]))?),
        PrimecountBelow => Value::Int(primecount(int(&args[0]))),
        PrevprimeBefore => Value::Int(prevprime(int(&args[0]))?),
        CoprimeAre => Value::Bool(gcd(int(&args[0]), int(&args[1])) == 1),
        AmicableAre => {
            let (a, b2) = (int(&args[0]), int(&args[1]));
            if a < 1 || b2 < 1 || a == b2 {
                Value::Bool(false)
            } else {
                Value::Bool((divisor_sum(a)? - a) == b2 && (divisor_sum(b2)? - b2) == a)
            }
        }
        MultiGcd3 => Value::Int(gcd(gcd(int(&args[0]), int(&args[1])), int(&args[2]))),
        MultiLcm3 => Value::Int(lcm3(int(&args[0]), int(&args[1]), int(&args[2]))?),
        A1z26Encode => string(a1z26_encode(text(&args[0]))),
        A1z26Decode => string(a1z26_decode(text(&args[0]))?),
        RevbitsOf => Value::Int(revbits(int(&args[0]), int(&args[1]))?),
        LuminanceOf => Value::Real(lum_perceived(text(&args[0]))?),
        ContrastRatio => {
            let (l1, l2) = (rel_lum(text(&args[0]))?, rel_lum(text(&args[1]))?);
            let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
            Value::Real((hi + 0.05) / (lo + 0.05))
        }
        SplitbillPerPerson => {
            let p = int(&args[1]);
            if p <= 0 {
                return Err("splitbill.per_person: there must be at least one person".to_string());
            }
            Value::Real(real(&args[0]) / p as f64)
        }
        ElapsedPercent => {
            let t = real(&args[1]);
            if t == 0.0 {
                return Err("elapsed.percent: the total is zero".to_string());
            }
            Value::Real(real(&args[0]) / t * 100.0)
        }

        // module "db" (SQLite)
        #[cfg(not(feature = "native"))]
        DbOpen | DbRun | DbValue | DbQuery | DbClose => return Err(DB_OFF.to_string()),
        #[cfg(feature = "native")]
        DbOpen => {
            let conn = rusqlite::Connection::open(text(&args[0])).map_err(|e| format!("db.open: {}", e))?;
            Value::Int(DBS.with(|d| {
                let mut d = d.borrow_mut();
                d.push(Some(conn));
                (d.len() - 1) as i64
            }))
        }
        #[cfg(feature = "native")]
        DbRun => {
            let n = db_with(int(&args[0]), |c| c.execute(text(&args[1]), []).map_err(|e| format!("db.run: {}", e)))?;
            Value::Int(n as i64)
        }
        #[cfg(feature = "native")]
        DbValue => db_with(int(&args[0]), |c| {
            let mut stmt = c.prepare(text(&args[1])).map_err(|e| format!("db.value: {}", e))?;
            let mut rows = stmt.query([]).map_err(|e| format!("db.value: {}", e))?;
            match rows.next().map_err(|e| format!("db.value: {}", e))? {
                Some(row) => Ok(string(sqlite_to_string(row.get_ref(0).map_err(|e| format!("db.value: {}", e))?))),
                None => Ok(string("")),
            }
        })?,
        #[cfg(feature = "native")]
        DbQuery => db_with(int(&args[0]), |c| {
            let mut stmt = c.prepare(text(&args[1])).map_err(|e| format!("db.query: {}", e))?;
            let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let mut rows = stmt.query([]).map_err(|e| format!("db.query: {}", e))?;
            let mut arr: Vec<Json> = Vec::new();
            while let Some(row) = rows.next().map_err(|e| format!("db.query: {}", e))? {
                let mut obj = serde_json::Map::new();
                for (i, name) in cols.iter().enumerate() {
                    let v = sqlite_to_json(row.get_ref(i).map_err(|e| format!("db.query: {}", e))?);
                    obj.insert(name.clone(), v);
                }
                arr.push(Json::Object(obj));
            }
            Ok(string(serde_json::to_string(&Json::Array(arr)).unwrap_or_default()))
        })?,
        #[cfg(feature = "native")]
        DbClose => {
            if let Ok(i) = usize::try_from(int(&args[0])) {
                DBS.with(|d| {
                    if let Some(slot) = d.borrow_mut().get_mut(i) {
                        *slot = None;
                    }
                });
            }
            Value::Null
        }

        // module "serve" (HTTP server)
        #[cfg(not(feature = "native"))]
        ServeStart | ServeWait | ServeReply | ServeStop => return Err(SERVE_OFF.to_string()),
        #[cfg(feature = "native")]
        ServeStart => {
            let port = int(&args[0]);
            if !(1..=65535).contains(&port) {
                return Err("serve.start: the port must be between 1 and 65535".to_string());
            }
            let server = tiny_http::Server::http(("127.0.0.1", port as u16)).map_err(|e| format!("serve.start: {}", e))?;
            Value::Int(SERVERS.with(|s| {
                let mut s = s.borrow_mut();
                s.push(Some(ServerState { server, pending: None }));
                (s.len() - 1) as i64
            }))
        }
        #[cfg(feature = "native")]
        ServeWait => {
            let json = SERVERS.with(|s| {
                let mut s = s.borrow_mut();
                let st = usize::try_from(int(&args[0]))
                    .ok()
                    .and_then(|i| s.get_mut(i))
                    .and_then(|o| o.as_mut())
                    .ok_or("serve.wait: this is not a running server")?;
                let mut req = st.server.recv().map_err(|e| format!("serve.wait: {}", e))?;
                let method = req.method().as_str().to_string();
                let path = req.url().to_string();
                let mut body = String::new();
                let _ = std::io::Read::read_to_string(req.as_reader(), &mut body);
                st.pending = Some(req);
                let mut m = serde_json::Map::new();
                m.insert("method".to_string(), Json::String(method));
                m.insert("path".to_string(), Json::String(path));
                m.insert("body".to_string(), Json::String(body));
                Ok::<String, String>(serde_json::to_string(&Json::Object(m)).unwrap_or_default())
            })?;
            string(json)
        }
        #[cfg(feature = "native")]
        ServeReply => {
            SERVERS.with(|s| {
                let mut s = s.borrow_mut();
                let st = usize::try_from(int(&args[0]))
                    .ok()
                    .and_then(|i| s.get_mut(i))
                    .and_then(|o| o.as_mut())
                    .ok_or("serve.reply: this is not a running server")?;
                let req = st.pending.take().ok_or("serve.reply: there is no request to reply to (call serve.wait first)")?;
                let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                    .expect("valid header");
                let resp = tiny_http::Response::from_string(text(&args[2]))
                    .with_status_code(int(&args[1]) as u16)
                    .with_header(header);
                req.respond(resp).map_err(|e| format!("serve.reply: {}", e))?;
                Ok::<(), String>(())
            })?;
            Value::Null
        }
        #[cfg(feature = "native")]
        ServeStop => {
            if let Ok(i) = usize::try_from(int(&args[0])) {
                SERVERS.with(|s| {
                    if let Some(slot) = s.borrow_mut().get_mut(i) {
                        *slot = None;
                    }
                });
            }
            Value::Null
        }

        // module "proc" (background processes)
        ProcStart => {
            let cmd = text(&args[0]);
            let mut c = if cfg!(windows) {
                let mut c = std::process::Command::new("cmd");
                c.arg("/C").arg(cmd);
                c
            } else {
                let mut c = std::process::Command::new("sh");
                c.arg("-c").arg(cmd);
                c
            };
            c.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
            let child = c.spawn().map_err(|e| format!("proc.start: cannot start the command ({})", e))?;
            Value::Int(PROCS.with(|p| {
                let mut p = p.borrow_mut();
                p.push(Some(child));
                (p.len() - 1) as i64
            }))
        }
        ProcDone => PROCS.with(|p| {
            let mut p = p.borrow_mut();
            match usize::try_from(int(&args[0])).ok().and_then(|i| p.get_mut(i)) {
                Some(Some(child)) => Ok(Value::Bool(child.try_wait().map_err(|e| format!("proc.done: {}", e))?.is_some())),
                Some(None) => Ok(Value::Bool(true)),
                None => Err("proc.done: this is not a running process".to_string()),
            }
        })?,
        ProcWait => {
            let child = PROCS.with(|p| {
                let mut p = p.borrow_mut();
                match usize::try_from(int(&args[0])).ok().and_then(|i| p.get_mut(i)) {
                    Some(slot @ Some(_)) => Ok(slot.take().expect("some")),
                    Some(None) => Err("proc.wait: this process was already waited for".to_string()),
                    None => Err("proc.wait: this is not a running process".to_string()),
                }
            })?;
            let out = child.wait_with_output().map_err(|e| format!("proc.wait: {}", e))?;
            if !out.status.success() {
                let code = out.status.code().map_or_else(|| "stopped by a signal".to_string(), |n| format!("exit code {}", n));
                return Err(format!("proc.wait: the command failed ({}): {}", code, String::from_utf8_lossy(&out.stderr).trim()));
            }
            string(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        ProcKill => {
            if let Ok(i) = usize::try_from(int(&args[0])) {
                PROCS.with(|p| {
                    if let Some(slot) = p.borrow_mut().get_mut(i) {
                        if let Some(mut child) = slot.take() {
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }
                });
            }
            Value::Null
        }
        HashSha256 => {
            use sha2::{Digest, Sha256};
            string(hex(Sha256::digest(text(&args[0]).as_bytes())))
        }
        HashSha512 => {
            use sha2::{Digest, Sha512};
            string(hex(Sha512::digest(text(&args[0]).as_bytes())))
        }
        WebGet => string(web_get(text(&args[0]))?),
        DataText => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))? {
                Json::String(s) => string(s.clone()),
                Json::Number(n) => string(n.to_string()),
                Json::Bool(b) => string(b.to_string()),
                _ => return Err(format!("the value at \"{}\" is not text", text(&args[1]))),
            }
        }
        DataNumber => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))?.as_f64() {
                Some(n) => Value::Real(n),
                None => return Err(format!("the value at \"{}\" is not a number", text(&args[1]))),
            }
        }
        DataFlag => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))?.as_bool() {
                Some(v) => Value::Bool(v),
                None => return Err(format!("the value at \"{}\" is not true or false", text(&args[1]))),
            }
        }
        DataHas => {
            let root = json_root(&args[0])?;
            Value::Bool(json_at(&root, text(&args[1])).is_ok())
        }
        DataSize => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))? {
                Json::Array(a) => Value::Int(a.len() as i64),
                Json::Object(o) => Value::Int(o.len() as i64),
                _ => return Err(format!("the value at \"{}\" is not a list or an object", text(&args[1]))),
            }
        }
        DataKeys => {
            let root = json_root(&args[0])?;
            match json_at(&root, text(&args[1]))? {
                Json::Object(o) => list(o.keys().map(|k| string(k.clone())).collect()),
                _ => return Err(format!("the value at \"{}\" is not an object", text(&args[1]))),
            }
        }
        DataToJson => string(serde_json::to_string(&to_json_value(&args[0])?).map_err(|e| format!("cannot make JSON: {}", e))?),
        ClockStamp => Value::Int(chrono::Utc::now().timestamp()),
        ClockFormat => string(clock_format(int(&args[0]), text(&args[1]))?),
        ClockParse => Value::Int(clock_parse(text(&args[0]), text(&args[1]))?),
        EncodeBase64 => {
            use base64::{engine::general_purpose::STANDARD, Engine};
            string(STANDARD.encode(text(&args[0]).as_bytes()))
        }
        EncodeFromBase64 => {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let bytes = STANDARD
                .decode(text(&args[0]))
                .map_err(|e| format!("encode.from_base64: not valid base64 ({})", e))?;
            string(String::from_utf8(bytes).map_err(|_| "encode.from_base64: the decoded data is not text".to_string())?)
        }
        ShellRun => string(shell_run(text(&args[0]))?),

        #[cfg(feature = "native")]
        FfiOpen => Value::Int(ffi_open(text(&args[0]))?),
        #[cfg(feature = "native")]
        FfiInt => {
            let xs: Vec<i64> = seq(&args[2]).borrow().iter().map(int).collect();
            Value::Int(ffi_call_int(int(&args[0]), text(&args[1]), &xs)?)
        }
        #[cfg(feature = "native")]
        FfiInt32 => {
            let xs: Vec<i64> = seq(&args[2]).borrow().iter().map(int).collect();
            Value::Int(ffi_call_int32(int(&args[0]), text(&args[1]), &xs)?)
        }
        #[cfg(feature = "native")]
        FfiReal => {
            let xs: Vec<f64> = seq(&args[2]).borrow().iter().map(real).collect();
            Value::Real(ffi_call_real(int(&args[0]), text(&args[1]), &xs)?)
        }
        #[cfg(feature = "native")]
        FfiCstr => Value::Int(ffi_cstr(text(&args[0]))?),
        #[cfg(feature = "native")]
        FfiRead => string(ffi_read(int(&args[0]))?),
        #[cfg(feature = "native")]
        FfiBuffer => Value::Int(floats(&args[0]).borrow().as_ptr() as i64),
        #[cfg(feature = "native")]
        FfiAlloc => Value::Int(ffi_alloc(int(&args[0]))?),
        #[cfg(feature = "native")]
        FfiPokeInt => {
            ffi_poke_int(int(&args[0]), int(&args[1]), int(&args[2]));
            Value::Null
        }
        #[cfg(feature = "native")]
        FfiPokeI32 => {
            ffi_poke_i32(int(&args[0]), int(&args[1]), int(&args[2]));
            Value::Null
        }
        #[cfg(feature = "native")]
        FfiPokeReal => {
            ffi_poke_real(int(&args[0]), int(&args[1]), real(&args[2]));
            Value::Null
        }
        #[cfg(feature = "native")]
        FfiPeekInt => Value::Int(ffi_peek_int(int(&args[0]), int(&args[1]))),
        #[cfg(feature = "native")]
        FfiPeekI32 => Value::Int(ffi_peek_i32(int(&args[0]), int(&args[1]))),
        #[cfg(feature = "native")]
        FfiPeekReal => Value::Real(ffi_peek_real(int(&args[0]), int(&args[1]))),
        #[cfg(feature = "ffi-full")]
        FfiCallInt | FfiCallReal => {
            let xs: Vec<f64> = seq(&args[3]).borrow().iter().map(real).collect();
            match ffi_call_sig(int(&args[0]), text(&args[1]), text(&args[2]), &xs, b == FfiCallInt)? {
                FfiRet::Int(n) => Value::Int(n),
                FfiRet::Real(x) => Value::Real(x),
            }
        }
        #[cfg(all(feature = "native", not(feature = "ffi-full")))]
        FfiCallInt | FfiCallReal => {
            return Err("ffi.call_int / ffi.call_real need a build with --features ffi-full".to_string())
        }
        #[cfg(not(feature = "native"))]
        FfiOpen | FfiInt | FfiInt32 | FfiReal | FfiCstr | FfiRead | FfiBuffer | FfiCallInt | FfiCallReal
        | FfiAlloc | FfiPokeInt | FfiPokeI32 | FfiPokeReal | FfiPeekInt | FfiPeekI32 | FfiPeekReal => {
            return Err("the ffi module is not available in the web playground".to_string())
        }

        // math
        MathGcd => Value::Int(gcd(int(&args[0]), int(&args[1]))),
        MathLcm => {
            let (x, y) = (int(&args[0]), int(&args[1]));
            let g = gcd(x, y);
            if g == 0 {
                Value::Int(0)
            } else {
                Value::Int((x / g).checked_mul(y).ok_or(OVERFLOW)?.abs())
            }
        }
        MathFactorial => {
            let n = int(&args[0]);
            if n < 0 {
                return Err("math.factorial: the number cannot be negative".to_string());
            }
            let mut acc: i64 = 1;
            for k in 2..=n {
                acc = acc.checked_mul(k).ok_or(OVERFLOW)?;
            }
            Value::Int(acc)
        }
        MathIsPrime => Value::Bool(is_prime(int(&args[0]))),
        MathSign => {
            let x = real(&args[0]);
            Value::Int(if x > 0.0 { 1 } else if x < 0.0 { -1 } else { 0 })
        }
        MathHypot => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        MathClamp => Value::Real(real(&args[0]).max(real(&args[1])).min(real(&args[2]))),
        MathCbrt => Value::Real(real(&args[0]).cbrt()),

        // bits
        BitsAnd => Value::Int(int(&args[0]) & int(&args[1])),
        BitsOr => Value::Int(int(&args[0]) | int(&args[1])),
        BitsXor => Value::Int(int(&args[0]) ^ int(&args[1])),
        BitsNot => Value::Int(!int(&args[0])),
        BitsLeft => Value::Int(int(&args[0]).wrapping_shl(int(&args[1]).clamp(0, 63) as u32)),
        BitsRight => Value::Int(int(&args[0]).wrapping_shr(int(&args[1]).clamp(0, 63) as u32)),
        BitsOnes => Value::Int(int(&args[0]).count_ones() as i64),

        // base and roman
        BaseTo => string(int_to_base(int(&args[0]), int(&args[1]))?),
        BaseFrom => Value::Int(base_from(text(&args[0]), int(&args[1]))?),
        RomanTo => string(roman_to(int(&args[0]))?),
        RomanFrom => Value::Int(roman_from(text(&args[0]))?),

        // temperatures, angles, percentages
        TempCToF => Value::Real(real(&args[0]) * 9.0 / 5.0 + 32.0),
        TempFToC => Value::Real((real(&args[0]) - 32.0) * 5.0 / 9.0),
        TempCToK => Value::Real(real(&args[0]) + 273.15),
        TempKToC => Value::Real(real(&args[0]) - 273.15),
        AngleToRadians => Value::Real(real(&args[0]).to_radians()),
        AngleToDegrees => Value::Real(real(&args[0]).to_degrees()),
        AngleNormalize => Value::Real(real(&args[0]).rem_euclid(360.0)),
        PercentOf => Value::Real(real(&args[0]) / 100.0 * real(&args[1])),
        PercentChange => {
            let (from, to) = (real(&args[0]), real(&args[1]));
            if from == 0.0 {
                return Err("percent.change: the starting value is zero".to_string());
            }
            Value::Real((to - from) / from * 100.0)
        }

        // geometry and units
        GeomCircleArea => Value::Real(std::f64::consts::PI * real(&args[0]).powi(2)),
        GeomRectArea => Value::Real(real(&args[0]) * real(&args[1])),
        GeomTriangleArea => Value::Real(real(&args[0]) * real(&args[1]) / 2.0),
        GeomSphereVolume => Value::Real(4.0 / 3.0 * std::f64::consts::PI * real(&args[0]).powi(3)),
        GeomCubeVolume => Value::Real(real(&args[0]).powi(3)),
        LengthMToFt => Value::Real(real(&args[0]) * 3.280_839_895),
        LengthFtToM => Value::Real(real(&args[0]) / 3.280_839_895),
        LengthKmToMi => Value::Real(real(&args[0]) * 0.621_371_192),
        LengthMiToKm => Value::Real(real(&args[0]) / 0.621_371_192),
        WeightKgToLb => Value::Real(real(&args[0]) * 2.204_622_622),
        WeightLbToKg => Value::Real(real(&args[0]) / 2.204_622_622),
        SizeHuman => string(size_human(int(&args[0]))),
        DurationHuman => string(duration_human(int(&args[0]))),
        GeoDistance => Value::Real(haversine(real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]))),
        TaxAdd => Value::Real(real(&args[0]) * (1.0 + real(&args[1]) / 100.0)),
        TaxRemove => Value::Real(real(&args[0]) / (1.0 + real(&args[1]) / 100.0)),
        CompassDirection => string(compass_direction(real(&args[0]))),

        // number helpers
        OrdinalOf => string(ordinal_of(int(&args[0]))),
        FibAt => Value::Int(fib_at(int(&args[0]))?),
        PrimeNext => Value::Int(prime_next(int(&args[0]))),
        PrimeNth => Value::Int(prime_nth(int(&args[0]))?),

        // strings
        StrRepeat => {
            let (s, n) = (text(&args[0]), int(&args[1]));
            if n < 0 {
                return Err("str.repeat: the count cannot be negative".to_string());
            }
            if s.len().saturating_mul(n as usize) > 50_000_000 {
                return Err("str.repeat: the result would be too large".to_string());
            }
            string(s.repeat(n as usize))
        }
        StrReverse => string(text(&args[0]).chars().rev().collect::<String>()),
        StrPadLeft => string(pad(text(&args[0]), int(&args[1]), text(&args[2]), true)),
        StrPadRight => string(pad(text(&args[0]), int(&args[1]), text(&args[2]), false)),
        StrCenter => string(center(text(&args[0]), int(&args[1]), text(&args[2]))),
        HexEncode => string(hex(text(&args[0]).as_bytes())),
        HexDecode => string(hex_decode(text(&args[0]))?),
        UrlEncode => string(url_encode(text(&args[0]))),
        UrlDecode => string(url_decode(text(&args[0]))?),
        CaseTitle => string(case_title(text(&args[0]))),
        CaseSnake => string(case_join(text(&args[0]), '_')),
        CaseKebab => string(case_join(text(&args[0]), '-')),
        CaseCamel => string(case_camel(text(&args[0]))),
        SlugMake => string(slug_make(text(&args[0]))),
        AsciiIsDigit => Value::Bool(all_chars(text(&args[0]), |c| c.is_ascii_digit())),
        AsciiIsAlpha => Value::Bool(all_chars(text(&args[0]), |c| c.is_ascii_alphabetic())),
        AsciiIsSpace => Value::Bool(all_chars(text(&args[0]), |c| c.is_ascii_whitespace())),
        AsciiIsUpper => Value::Bool(all_cased(text(&args[0]), true)),
        AsciiIsLower => Value::Bool(all_cased(text(&args[0]), false)),
        MorseEncode => string(morse_encode(text(&args[0]))?),
        MorseDecode => string(morse_decode(text(&args[0]))?),
        CaesarEncode => string(caesar(text(&args[0]), int(&args[1]))),
        CaesarDecode => string(caesar(text(&args[0]), -int(&args[1]))),
        Rot13Apply => string(caesar(text(&args[0]), 13)),
        LeetEncode => string(leet_encode(text(&args[0]))),
        NatoEncode => string(nato_encode(text(&args[0]))),

        // colors and validation
        ColorHex => {
            let c = |v: &Value| int(v).clamp(0, 255);
            string(format!("#{:02x}{:02x}{:02x}", c(&args[0]), c(&args[1]), c(&args[2])))
        }
        ColorGray => {
            let v = int(&args[0]).clamp(0, 255);
            string(format!("#{:02x}{:02x}{:02x}", v, v, v))
        }
        ColorInvert => string(color_invert(text(&args[0]))?),
        ValidEmail => Value::Bool(regex_is(r"^[^@\s]+@[^@\s]+\.[^@\s]+$", text(&args[0]))),
        ValidInteger => Value::Bool(regex_is(r"^[+-]?[0-9]+$", text(&args[0]).trim())),
        ValidNumber => Value::Bool(regex_is(r"^[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)$", text(&args[0]).trim())),
        LuhnValid => Value::Bool(luhn_valid(text(&args[0]))),
        IsbnValid => Value::Bool(isbn_valid(text(&args[0]))),
        UuidMake => string(uuid::Uuid::new_v4().to_string()),
        CrcValue => Value::Int(i64::from(crc32fast::hash(text(&args[0]).as_bytes()))),

        // environment, paths and the operating system
        EnvGet => string(std::env::var(text(&args[0])).unwrap_or_default()),
        EnvHas => Value::Bool(std::env::var_os(text(&args[0])).is_some()),
        EnvSet => {
            std::env::set_var(text(&args[0]), text(&args[1]));
            Value::Null
        }
        PathJoin => string(std::path::Path::new(text(&args[0])).join(text(&args[1])).to_string_lossy().into_owned()),
        PathName => string(path_part(text(&args[0]), PathPart::Name)),
        PathStem => string(path_part(text(&args[0]), PathPart::Stem)),
        PathExtension => string(path_part(text(&args[0]), PathPart::Ext)),
        PathParent => string(path_part(text(&args[0]), PathPart::Parent)),
        PathIsAbsolute => Value::Bool(std::path::Path::new(text(&args[0])).is_absolute()),
        OsName => string(std::env::consts::OS),
        OsArch => string(std::env::consts::ARCH),
        OsFamily => string(std::env::consts::FAMILY),
        DirList => {
            let path = text(&args[0]);
            let mut names = Vec::new();
            for entry in std::fs::read_dir(path).map_err(|e| format!("dir.list: cannot read \"{}\": {}", path, e))? {
                let entry = entry.map_err(|e| format!("dir.list: {}", e))?;
                names.push(string(entry.file_name().to_string_lossy().into_owned()));
            }
            list(names)
        }
        DirMake => {
            let p = text(&args[0]);
            std::fs::create_dir_all(p).map_err(|e| format!("dir.make: cannot create \"{}\": {}", p, e))?;
            Value::Null
        }
        DirCurrent => string(std::env::current_dir().map_err(|e| format!("dir.current: {}", e))?.to_string_lossy().into_owned()),
        DirExists => Value::Bool(std::path::Path::new(text(&args[0])).is_dir()),
        FileSize => {
            let p = text(&args[0]);
            let m = std::fs::metadata(p).map_err(|e| format!("file.size: cannot read \"{}\": {}", p, e))?;
            Value::Int(m.len() as i64)
        }
        FileCopy => {
            std::fs::copy(text(&args[0]), text(&args[1])).map_err(|e| format!("file.copy: {}", e))?;
            Value::Null
        }
        FileRename => {
            std::fs::rename(text(&args[0]), text(&args[1])).map_err(|e| format!("file.rename: {}", e))?;
            Value::Null
        }
        FileModified => {
            let p = text(&args[0]);
            let m = std::fs::metadata(p).map_err(|e| format!("file.modified: {}", e))?;
            let t = m.modified().map_err(|e| format!("file.modified: {}", e))?;
            Value::Int(t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0))
        }
        FileDelete => {
            let p = text(&args[0]);
            std::fs::remove_file(p).map_err(|e| format!("file.delete: cannot delete \"{}\": {}", p, e))?;
            Value::Null
        }
        TmpPath => string(std::env::temp_dir().to_string_lossy().into_owned()),
        ArgsCount => Value::Int(program_args().len() as i64),
        ArgsGet => {
            let a = program_args();
            let i = int(&args[0]);
            if i < 1 || i as usize > a.len() {
                return Err(format!("args.get: there is no argument number {}", i));
            }
            string(a[i as usize - 1].clone())
        }

        // CSV and statistics
        CsvFields => list(csv_fields(text(&args[0])).into_iter().map(string).collect()),
        CsvLine => {
            let items = seq(&args[0]).borrow();
            string(csv_line(items.iter().map(display)))
        }
        StatsMean => {
            let v = numbers(&args[0])?;
            Value::Real(v.iter().sum::<f64>() / v.len() as f64)
        }
        StatsMedian => {
            let mut v = numbers(&args[0])?;
            v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            let n = v.len();
            Value::Real(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
        }
        StatsStdev => {
            let v = numbers(&args[0])?;
            let m = v.iter().sum::<f64>() / v.len() as f64;
            let var = v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64;
            Value::Real(var.sqrt())
        }
        // batch A: unit conversions
        AreaSqmToSqft => Value::Real(real(&args[0]) * 10.763_910_4),
        AreaSqftToSqm => Value::Real(real(&args[0]) / 10.763_910_4),
        AreaAcreToSqm => Value::Real(real(&args[0]) * 4046.856_42),
        AreaSqmToAcre => Value::Real(real(&args[0]) / 4046.856_42),
        AreaHectareToAcre => Value::Real(real(&args[0]) * 2.471_053_8),
        AreaAcreToHectare => Value::Real(real(&args[0]) / 2.471_053_8),
        VolLToGal => Value::Real(real(&args[0]) * 0.264_172_05),
        VolGalToL => Value::Real(real(&args[0]) / 0.264_172_05),
        VolLToMl => Value::Real(real(&args[0]) * 1000.0),
        VolMlToL => Value::Real(real(&args[0]) / 1000.0),
        VolCupToMl => Value::Real(real(&args[0]) * 236.588_24),
        VolMlToCup => Value::Real(real(&args[0]) / 236.588_24),
        SpeedKmhToMph => Value::Real(real(&args[0]) * 0.621_371_19),
        SpeedMphToKmh => Value::Real(real(&args[0]) / 0.621_371_19),
        SpeedMpsToKmh => Value::Real(real(&args[0]) * 3.6),
        SpeedKmhToMps => Value::Real(real(&args[0]) / 3.6),
        SpeedKnotToKmh => Value::Real(real(&args[0]) * 1.852),
        SpeedKmhToKnot => Value::Real(real(&args[0]) / 1.852),
        PaToBar => Value::Real(real(&args[0]) / 100_000.0),
        BarToPa => Value::Real(real(&args[0]) * 100_000.0),
        PsiToBar => Value::Real(real(&args[0]) * 0.068_947_57),
        BarToPsi => Value::Real(real(&args[0]) / 0.068_947_57),
        AtmToBar => Value::Real(real(&args[0]) * 1.013_25),
        BarToAtm => Value::Real(real(&args[0]) / 1.013_25),
        JouleToCal => Value::Real(real(&args[0]) * 0.239_005_7),
        CalToJoule => Value::Real(real(&args[0]) / 0.239_005_7),
        KwhToJoule => Value::Real(real(&args[0]) * 3_600_000.0),
        JouleToKwh => Value::Real(real(&args[0]) / 3_600_000.0),
        WattToHp => Value::Real(real(&args[0]) * 0.001_341_022),
        HpToWatt => Value::Real(real(&args[0]) / 0.001_341_022),
        MpgToL100 => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("fuel.mpg_to_l100km: miles per gallon is zero".to_string()); }
            Value::Real(235.214_583 / x)
        }
        L100ToMpg => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("fuel.l100km_to_mpg: liters per 100km is zero".to_string()); }
            Value::Real(235.214_583 / x)
        }
        NewtonToLbf => Value::Real(real(&args[0]) * 0.224_808_94),
        LbfToNewton => Value::Real(real(&args[0]) / 0.224_808_94),
        NmToLbft => Value::Real(real(&args[0]) * 0.737_562_15),
        LbftToNm => Value::Real(real(&args[0]) / 0.737_562_15),
        MbpsToMbs => Value::Real(real(&args[0]) / 8.0),
        MbsToMbps => Value::Real(real(&args[0]) * 8.0),
        HzToRpm => Value::Real(real(&args[0]) * 60.0),
        RpmToHz => Value::Real(real(&args[0]) / 60.0),
        TbspToMl => Value::Real(real(&args[0]) * 14.786_765),
        MlToTbsp => Value::Real(real(&args[0]) / 14.786_765),
        TspToMl => Value::Real(real(&args[0]) * 4.928_922),
        MlToTsp => Value::Real(real(&args[0]) / 4.928_922),
        NmiToKm => Value::Real(real(&args[0]) * 1.852),
        KmToNmi => Value::Real(real(&args[0]) / 1.852),
        AuToKm => Value::Real(real(&args[0]) * 149_597_870.7),
        KmToAu => Value::Real(real(&args[0]) / 149_597_870.7),
        LyToKm => Value::Real(real(&args[0]) * 9.460_730_472e12),
        KmToLy => Value::Real(real(&args[0]) / 9.460_730_472e12),
        StoneToKg => Value::Real(real(&args[0]) * 6.350_293_18),
        KgToStone => Value::Real(real(&args[0]) / 6.350_293_18),
        Gcm3ToKgm3 => Value::Real(real(&args[0]) * 1000.0),
        Kgm3ToGcm3 => Value::Real(real(&args[0]) / 1000.0),
        GainToDb => Value::Real(20.0 * real(&args[0]).log10()),
        DbToGain => Value::Real(10f64.powf(real(&args[0]) / 20.0)),
        BpmToMs => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("tempo.bpm_to_ms: the tempo is zero".to_string()); }
            Value::Real(60_000.0 / x)
        }
        MsToBpm => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("tempo.ms_to_bpm: the duration is zero".to_string()); }
            Value::Real(60_000.0 / x)
        }
        DegToGrad => Value::Real(real(&args[0]) * 10.0 / 9.0),
        GradToDeg => Value::Real(real(&args[0]) * 9.0 / 10.0),
        RadToGrad => Value::Real(real(&args[0]) * 200.0 / std::f64::consts::PI),
        GradToRad => Value::Real(real(&args[0]) * std::f64::consts::PI / 200.0),
        HoursToSeconds => Value::Real(real(&args[0]) * 3600.0),
        SecondsToHours => Value::Real(real(&args[0]) / 3600.0),
        DaysToSeconds => Value::Real(real(&args[0]) * 86400.0),
        SecondsToDays => Value::Real(real(&args[0]) / 86400.0),
        MinkmToKmh => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("pace.minkm_to_kmh: the pace is zero".to_string()); }
            Value::Real(60.0 / x)
        }
        KmhToMinkm => {
            let x = real(&args[0]);
            if x == 0.0 { return Err("pace.kmh_to_minkm: the speed is zero".to_string()); }
            Value::Real(60.0 / x)
        }

        // batch B: maths
        HyperSinh => Value::Real(real(&args[0]).sinh()),
        HyperCosh => Value::Real(real(&args[0]).cosh()),
        HyperTanh => Value::Real(real(&args[0]).tanh()),
        AhyperAsinh => Value::Real(real(&args[0]).asinh()),
        AhyperAcosh => Value::Real(real(&args[0]).acosh()),
        AhyperAtanh => Value::Real(real(&args[0]).atanh()),
        LogsTwo => Value::Real(real(&args[0]).log2()),
        LogsTen => Value::Real(real(&args[0]).log10()),
        LogsNatural => Value::Real(real(&args[0]).ln()),
        TrigxSec => Value::Real(1.0 / real(&args[0]).to_radians().cos()),
        TrigxCsc => Value::Real(1.0 / real(&args[0]).to_radians().sin()),
        TrigxCot => Value::Real(1.0 / real(&args[0]).to_radians().tan()),
        ChooseCombinations => Value::Int(combinations(int(&args[0]), int(&args[1]))?),
        ChoosePermutations => Value::Int(permutations(int(&args[0]), int(&args[1]))?),
        SeqTriangular => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n + 1).ok_or(OVERFLOW)? / 2)
        }
        SeqSquare => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n).ok_or(OVERFLOW)?)
        }
        SeqCube => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n).and_then(|x| x.checked_mul(n)).ok_or(OVERFLOW)?)
        }
        SeqCatalan => Value::Int(catalan(int(&args[0]))?),
        SeqTetrahedral => {
            let n = int(&args[0]);
            Value::Int(n.checked_mul(n + 1).and_then(|x| x.checked_mul(n + 2)).ok_or(OVERFLOW)? / 6)
        }
        ModularPow => Value::Int(mod_pow(int(&args[0]), int(&args[1]), int(&args[2]))?),
        LerpBetween => {
            let (a, c, t) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(a + (c - a) * t)
        }
        LerpMapRange => {
            let (x, a, b2, c, d) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
            if a == b2 {
                return Err("lerp.map_range: the input range is empty".to_string());
            }
            Value::Real(c + (x - a) * (d - c) / (b2 - a))
        }
        RoundingUp => to_int_checked(real(&args[0]).ceil())?,
        RoundingDown => to_int_checked(real(&args[0]).floor())?,
        RoundingHalfUp => to_int_checked((real(&args[0]) + 0.5).floor())?,
        RoundingToMultiple => {
            let (x, m) = (real(&args[0]), real(&args[1]));
            if m == 0.0 {
                return Err("rounding.to_multiple: the step is zero".to_string());
            }
            Value::Real((x / m).round() * m)
        }
        Vec2Length => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        Vec2Dot => Value::Real(real(&args[0]) * real(&args[2]) + real(&args[1]) * real(&args[3])),
        Vec2Angle => Value::Real(real(&args[1]).atan2(real(&args[0])).to_degrees()),
        Vec3Length => {
            let (x, y, z) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real((x * x + y * y + z * z).sqrt())
        }
        Vec3Dot => Value::Real(
            real(&args[0]) * real(&args[3]) + real(&args[1]) * real(&args[4]) + real(&args[2]) * real(&args[5]),
        ),
        ComplexAbs => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        ComplexArg => Value::Real(real(&args[1]).atan2(real(&args[0]))),
        ParityIsEven => Value::Bool(int(&args[0]) % 2 == 0),
        ParityIsOdd => Value::Bool(int(&args[0]) % 2 != 0),
        DivisibleBy => {
            let d = int(&args[1]);
            if d == 0 {
                return Err("divisible.by: cannot divide by zero".to_string());
            }
            Value::Bool(int(&args[0]) % d == 0)
        }
        DigitsSum => Value::Int(int(&args[0]).unsigned_abs().to_string().bytes().map(|b| (b - b'0') as i64).sum()),
        DigitsCount => Value::Int(int(&args[0]).unsigned_abs().to_string().len() as i64),
        DigitsReverse => {
            let n = int(&args[0]);
            let s: String = n.unsigned_abs().to_string().chars().rev().collect();
            let v: i64 = s.parse().map_err(|_| OVERFLOW.to_string())?;
            Value::Int(if n < 0 { -v } else { v })
        }
        CollatzSteps => Value::Int(collatz(int(&args[0]))?),
        PerfectIs => Value::Bool(is_perfect(int(&args[0]))),
        ArmstrongIs => Value::Bool(is_armstrong(int(&args[0]))),
        PalindromeText => {
            let clean: String =
                text(&args[0]).chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
            Value::Bool(clean.chars().eq(clean.chars().rev()))
        }
        PalindromeNumber => {
            let s = int(&args[0]).unsigned_abs().to_string();
            Value::Bool(s.chars().eq(s.chars().rev()))
        }
        RangeContains => {
            let (x, lo, hi) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Bool(x >= lo && x <= hi)
        }
        PointDistance => {
            let (x1, y1, x2, y2) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
            Value::Real((x2 - x1).hypot(y2 - y1))
        }
        PointManhattan => {
            Value::Real((real(&args[2]) - real(&args[0])).abs() + (real(&args[3]) - real(&args[1])).abs())
        }
        SlopeBetween => {
            let (x1, y1, x2, y2) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
            if x1 == x2 {
                return Err("slope.between: the line is vertical".to_string());
            }
            Value::Real((y2 - y1) / (x2 - x1))
        }
        RootSquare => Value::Real(real(&args[0]).sqrt()),
        RootCube => Value::Real(real(&args[0]).cbrt()),
        RootNth => {
            let (x, n) = (real(&args[0]), real(&args[1]));
            if n == 0.0 {
                return Err("root.nth: the index is zero".to_string());
            }
            Value::Real(x.powf(1.0 / n))
        }

        // batch C: more text
        CountWords => Value::Int(text(&args[0]).split_whitespace().count() as i64),
        CountLines => {
            let s = text(&args[0]);
            Value::Int(if s.is_empty() { 0 } else { s.lines().count() as i64 })
        }
        CountChars => Value::Int(text(&args[0]).chars().count() as i64),
        CountVowels => Value::Int(text(&args[0]).chars().filter(|c| "aeiouAEIOU".contains(*c)).count() as i64),
        TruncateTo => {
            let (s, n) = (text(&args[0]), int(&args[1]).max(0) as usize);
            if s.chars().count() <= n {
                string(s)
            } else {
                string(s.chars().take(n).collect::<String>() + "…")
            }
        }
        TruncateWords => {
            let (s, n) = (text(&args[0]), int(&args[1]).max(0) as usize);
            let ws: Vec<&str> = s.split_whitespace().collect();
            if ws.len() <= n {
                string(s)
            } else {
                string(ws[..n].join(" ") + " …")
            }
        }
        IndentBy => {
            let (s, n) = (text(&args[0]), int(&args[1]).max(0) as usize);
            let pad = " ".repeat(n);
            string(s.lines().map(|l| format!("{}{}", pad, l)).collect::<Vec<_>>().join("\n"))
        }
        StripPrefix => {
            let (s, p) = (text(&args[0]), text(&args[1]));
            string(s.strip_prefix(p).unwrap_or(s))
        }
        StripSuffix => {
            let (s, p) = (text(&args[0]), text(&args[1]));
            string(s.strip_suffix(p).unwrap_or(s))
        }
        MaskAll => string(text(&args[0]).chars().map(|_| '*').collect::<String>()),
        MaskMiddle => string(mask_middle(text(&args[0]), int(&args[1]))),
        InitialsOf | AcronymOf => string(
            text(&args[0]).split_whitespace().filter_map(|w| w.chars().next()).map(|c| c.to_ascii_uppercase()).collect::<String>(),
        ),
        PigLatin => string(pig_latin(text(&args[0]))),
        AtbashApply => string(atbash(text(&args[0]))),
        VigenereEncode => string(vigenere(text(&args[0]), text(&args[1]), true)?),
        VigenereDecode => string(vigenere(text(&args[0]), text(&args[1]), false)?),
        Rot47Apply => string(rot47(text(&args[0]))),
        BinEncode => string(text(&args[0]).bytes().map(|b| format!("{:08b}", b)).collect::<Vec<_>>().join(" ")),
        BinDecode => string(bin_decode(text(&args[0]))?),
        Base32Encode => string(base32_encode(text(&args[0]).as_bytes())),
        Base32Decode => string(base32_decode(text(&args[0]))?),
        HtmlEscape => string(html_escape(text(&args[0]))),
        HtmlUnescape => string(html_unescape(text(&args[0]))),
        ShoutIt => string(format!("{}!", text(&args[0]).to_uppercase())),
        ShoutWhisper => string(text(&args[0]).to_lowercase()),
        WordsReverse => string(text(&args[0]).split_whitespace().rev().collect::<Vec<_>>().join(" ")),
        WordsFirst => string(text(&args[0]).split_whitespace().next().unwrap_or("")),
        WordsLast => string(text(&args[0]).split_whitespace().last().unwrap_or("")),
        UnicodeCodepoint => match text(&args[0]).chars().next() {
            Some(c) => Value::Int(c as i64),
            None => return Err("unicode.codepoint: the string is empty".to_string()),
        },
        UnicodeFromCodepoint => {
            let n = int(&args[0]);
            let c = u32::try_from(n)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("unicode.from_codepoint: {} is not a character", n))?;
            string(c.to_string())
        }
        UnicodeSize => Value::Int(text(&args[0]).chars().count() as i64),
        NormalizeSpaces => string(text(&args[0]).split_whitespace().collect::<Vec<_>>().join(" ")),
        AnagramOf => Value::Bool(anagram(text(&args[0]), text(&args[1]))),
        FizzbuzzAt => {
            let n = int(&args[0]);
            string(match (n % 3, n % 5) {
                (0, 0) => "FizzBuzz".to_string(),
                (0, _) => "Fizz".to_string(),
                (_, 0) => "Buzz".to_string(),
                _ => n.to_string(),
            })
        }
        QuoteAdd => string(format!("\"{}\"", text(&args[0]))),
        RleEncode => string(rle_encode(text(&args[0]))),
        RleDecode => string(rle_decode(text(&args[0]))?),
        WrapAt => string(wrap_at(text(&args[0]), int(&args[1]))),
        SpongebobMock => string(spongebob(text(&args[0]))),
        ClapText => string(text(&args[0]).split_whitespace().collect::<Vec<_>>().join(" 👏 ")),

        // batch D: science, finance, geometry, dates
        BmiValue => {
            let (w, h) = (real(&args[0]), real(&args[1]));
            if h <= 0.0 {
                return Err("bmi.value: the height must be positive".to_string());
            }
            Value::Real(w / (h * h))
        }
        BmiCategory => string(bmi_category(real(&args[0]))),
        OhmVoltage => Value::Real(real(&args[0]) * real(&args[1])),
        OhmCurrent => {
            let r = real(&args[1]);
            if r == 0.0 {
                return Err("ohm.current: the resistance is zero".to_string());
            }
            Value::Real(real(&args[0]) / r)
        }
        OhmResistance => {
            let i = real(&args[1]);
            if i == 0.0 {
                return Err("ohm.resistance: the current is zero".to_string());
            }
            Value::Real(real(&args[0]) / i)
        }
        OhmPower => Value::Real(real(&args[0]) * real(&args[1])),
        PhysicsForce => Value::Real(real(&args[0]) * real(&args[1])),
        PhysicsKinetic => {
            let (m, v) = (real(&args[0]), real(&args[1]));
            Value::Real(0.5 * m * v * v)
        }
        PhysicsMomentum => Value::Real(real(&args[0]) * real(&args[1])),
        WaveFrequency => {
            let wl = real(&args[1]);
            if wl == 0.0 {
                return Err("wave.frequency: the wavelength is zero".to_string());
            }
            Value::Real(real(&args[0]) / wl)
        }
        WaveWavelength => {
            let f = real(&args[1]);
            if f == 0.0 {
                return Err("wave.wavelength: the frequency is zero".to_string());
            }
            Value::Real(real(&args[0]) / f)
        }
        WavePeriod => {
            let f = real(&args[0]);
            if f == 0.0 {
                return Err("wave.period: the frequency is zero".to_string());
            }
            Value::Real(1.0 / f)
        }
        GravityOn => Value::Real(planet_gravity(text(&args[0]))?),
        InterestSimple => Value::Real(real(&args[0]) * real(&args[1]) / 100.0 * real(&args[2])),
        InterestCompound => {
            let (p, r, t) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(p * (1.0 + r / 100.0).powf(t) - p)
        }
        LoanPayment => Value::Real(loan_payment(real(&args[0]), real(&args[1]), int(&args[2]))?),
        TipAmount => Value::Real(real(&args[0]) * real(&args[1]) / 100.0),
        TipTotal => Value::Real(real(&args[0]) * (1.0 + real(&args[1]) / 100.0)),
        DiscountApply => Value::Real(real(&args[0]) * (1.0 - real(&args[1]) / 100.0)),
        DiscountSaved => Value::Real(real(&args[0]) * real(&args[1]) / 100.0),
        RoiPercent => {
            let c = real(&args[1]);
            if c == 0.0 {
                return Err("roi.percent: the cost is zero".to_string());
            }
            Value::Real(real(&args[0]) / c * 100.0)
        }
        MoneyFormat => string(money_format(real(&args[0]))),
        BearingBetween => Value::Real(bearing(real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]))),
        TriangleHypotenuse => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        TriangleHeron => Value::Real(heron(real(&args[0]), real(&args[1]), real(&args[2]))?),
        TriangleIsRight => {
            let mut s = [real(&args[0]), real(&args[1]), real(&args[2])];
            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            Value::Bool((s[0] * s[0] + s[1] * s[1] - s[2] * s[2]).abs() < 1e-9)
        }
        CircleArea => Value::Real(std::f64::consts::PI * real(&args[0]).powi(2)),
        CircleCircumference => Value::Real(2.0 * std::f64::consts::PI * real(&args[0])),
        CircleDiameter => Value::Real(2.0 * real(&args[0])),
        RectangleArea => Value::Real(real(&args[0]) * real(&args[1])),
        RectanglePerimeter => Value::Real(2.0 * (real(&args[0]) + real(&args[1]))),
        RectangleDiagonal => Value::Real(real(&args[0]).hypot(real(&args[1]))),
        SphereVolume => Value::Real(4.0 / 3.0 * std::f64::consts::PI * real(&args[0]).powi(3)),
        SphereSurface => Value::Real(4.0 * std::f64::consts::PI * real(&args[0]).powi(2)),
        CylinderVolume => {
            let (r, h) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * r * r * h)
        }
        CylinderSurface => {
            let (r, h) = (real(&args[0]), real(&args[1]));
            Value::Real(2.0 * std::f64::consts::PI * r * (r + h))
        }
        ConeVolume => {
            let (r, h) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * r * r * h / 3.0)
        }
        ConeSurface => {
            let (r, h) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * r * (r + (r * r + h * h).sqrt()))
        }
        WeekdayOf => string(weekday_of(int(&args[0]))?),
        MonthName => string(month_name(int(&args[0]))?),
        MonthDays => Value::Int(days_in_month(int(&args[0]), int(&args[1]))?),
        LeapYear => Value::Bool(is_leap(int(&args[0]))),
        AgeYears => Value::Int(age_years(int(&args[0]), int(&args[1]))),
        ZodiacSign => string(zodiac_sign(int(&args[0]), int(&args[1]))?),
        SeasonOf => string(season_of(int(&args[0]))?),
        GradeLetter => string(grade_letter(real(&args[0]))),
        LogicXor => Value::Bool(boolv(&args[0]) ^ boolv(&args[1])),
        LogicImplies => Value::Bool(!boolv(&args[0]) || boolv(&args[1])),
        LogicNand => Value::Bool(!(boolv(&args[0]) && boolv(&args[1]))),
        LogicNor => Value::Bool(!(boolv(&args[0]) || boolv(&args[1]))),
        YesnoOf => string(if boolv(&args[0]) { "yes" } else { "no" }),
        ScientificNotation => string(format!("{:e}", real(&args[0]))),
        FractionToDecimal => {
            let d = int(&args[1]);
            if d == 0 {
                return Err("fraction.to_decimal: the denominator is zero".to_string());
            }
            Value::Real(int(&args[0]) as f64 / d as f64)
        }
        FractionSimplify => string(fraction_simplify(int(&args[0]), int(&args[1]))?),
        HeartrateMax => Value::Int((220 - int(&args[0])).max(0)),
        HeartrateZone => {
            let (age, pct) = (int(&args[0]), int(&args[1]));
            Value::Int((220 - age).max(0) * pct / 100)
        }

        // batch E: hashing, validation, randomness, ids, utilities
        FnvHash => Value::Int(fnv1a(text(&args[0]))),
        Djb2Hash => Value::Int(djb2(text(&args[0]))),
        SdbmHash => Value::Int(sdbm(text(&args[0]))),
        AdlerValue => Value::Int(adler32(text(&args[0]))),
        Ipv4Valid => Value::Bool(ipv4_valid(text(&args[0]))),
        MacValid => Value::Bool(regex_is(r"^([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}$", text(&args[0]))),
        HexcolorValid => Value::Bool(regex_is(r"^#?([0-9A-Fa-f]{3}|[0-9A-Fa-f]{6})$", text(&args[0]))),
        WeburlValid => Value::Bool(regex_is(r"^https?://[^\s]+$", text(&args[0]))),
        DiceRoll => {
            let s = int(&args[0]);
            if s < 1 {
                return Err("dice.roll: the die needs at least 1 side".to_string());
            }
            Value::Int(rng_range(1, s))
        }
        DiceMany => {
            let (count, sides) = (int(&args[0]), int(&args[1]));
            if count < 0 {
                return Err("dice.many: the count cannot be negative".to_string());
            }
            if sides < 1 {
                return Err("dice.many: the die needs at least 1 side".to_string());
            }
            let mut total = 0i64;
            for _ in 0..count {
                total += rng_range(1, sides);
            }
            Value::Int(total)
        }
        CoinFlip => Value::Bool(rng_u64() & 1 == 0),
        PickBetween => {
            let (lo, hi) = (int(&args[0]), int(&args[1]));
            if lo > hi {
                return Err("pick.between: the minimum is greater than the maximum".to_string());
            }
            Value::Int(rng_range(lo, hi))
        }
        PickChance => Value::Bool((rng_u64() as f64 / u64::MAX as f64) < real(&args[0])),
        TokenHex => string(rng_hex(int(&args[0]))?),
        PasswordMake => string(rng_password(int(&args[0]))?),
        NanoidMake => string(rng_nanoid()),
        OtpCode => Value::Int(rng_range(100_000, 999_999)),
        LoremWords => string(lorem_words(int(&args[0]))),
        RandcolorHex => string(format!("#{:06x}", rng_u64() as u32 & 0xFF_FFFF)),
        FactorCount => Value::Int(divisor_count(int(&args[0]))?),
        FactorSum => Value::Int(divisor_sum(int(&args[0]))?),
        TotientOf => Value::Int(totient(int(&args[0]))?),
        CompareOf => {
            let (a, b) = (real(&args[0]), real(&args[1]));
            Value::Int(if a < b { -1 } else if a > b { 1 } else { 0 })
        }
        ClampintBetween => {
            let (x, lo, hi) = (int(&args[0]), int(&args[1]), int(&args[2]));
            Value::Int(x.max(lo).min(hi))
        }
        PowSquare => Value::Real(real(&args[0]).powi(2)),
        PowCube => Value::Real(real(&args[0]).powi(3)),
        PowOf => Value::Real(real(&args[0]).powf(real(&args[1]))),
        CapsFirst => string(caps_first(text(&args[0]))),
        SwapcaseApply => string(
            text(&args[0])
                .chars()
                .map(|c| if c.is_uppercase() { c.to_ascii_lowercase() } else { c.to_ascii_uppercase() })
                .collect::<String>(),
        ),
        ConsonantsCount => Value::Int(
            text(&args[0]).chars().filter(|c| c.is_ascii_alphabetic() && !"aeiouAEIOU".contains(*c)).count() as i64,
        ),
        Point3Distance => {
            let d = |a: usize, b: usize| real(&args[b]) - real(&args[a]);
            Value::Real((d(0, 3).powi(2) + d(1, 4).powi(2) + d(2, 5).powi(2)).sqrt())
        }
        MidpointOf => Value::Real((real(&args[0]) + real(&args[1])) / 2.0),
        IsqrtOf => {
            let n = int(&args[0]);
            if n < 0 {
                return Err("isqrt.of: the number is negative".to_string());
            }
            let mut r = (n as f64).sqrt() as i64;
            while r > 0 && r.saturating_mul(r) > n {
                r -= 1;
            }
            while (r + 1).saturating_mul(r + 1) <= n {
                r += 1;
            }
            Value::Int(r)
        }
        CycleIndex => {
            let len = int(&args[1]);
            if len <= 0 {
                return Err("cycle.index: the length must be positive".to_string());
            }
            Value::Int(int(&args[0]).rem_euclid(len))
        }
        DigitrootOf => {
            let n = int(&args[0]).unsigned_abs();
            Value::Int(if n == 0 { 0 } else { 1 + ((n - 1) % 9) as i64 })
        }

        // batch F: advanced geometry, finance, weather, physics
        TrapezoidArea => Value::Real((real(&args[0]) + real(&args[1])) / 2.0 * real(&args[2])),
        EllipseArea => Value::Real(std::f64::consts::PI * real(&args[0]) * real(&args[1])),
        EllipsePerimeter => {
            let (a, b) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * (3.0 * (a + b) - ((3.0 * a + b) * (a + 3.0 * b)).sqrt()))
        }
        PentagonArea => {
            let s = real(&args[0]);
            Value::Real(5.0 * s * s / (4.0 * (std::f64::consts::PI / 5.0).tan()))
        }
        HexagonArea => {
            let s = real(&args[0]);
            Value::Real(3.0 * 3f64.sqrt() / 2.0 * s * s)
        }
        PolygonInteriorAngle => {
            let n = int(&args[0]);
            if n < 3 {
                return Err("polygon.interior_angle: a polygon needs at least 3 sides".to_string());
            }
            Value::Real((n - 2) as f64 * 180.0 / n as f64)
        }
        PolygonArea => {
            let n = int(&args[0]);
            let s = real(&args[1]);
            if n < 3 {
                return Err("polygon.area: a polygon needs at least 3 sides".to_string());
            }
            Value::Real(n as f64 * s * s / (4.0 * (std::f64::consts::PI / n as f64).tan()))
        }
        PyramidVolume => Value::Real(real(&args[0]) * real(&args[1]) / 3.0),
        PrismVolume => Value::Real(real(&args[0]) * real(&args[1])),
        TorusVolume => {
            let (big, small) = (real(&args[0]), real(&args[1]));
            Value::Real(2.0 * std::f64::consts::PI.powi(2) * big * small * small)
        }
        CuboidVolume => Value::Real(real(&args[0]) * real(&args[1]) * real(&args[2])),
        CuboidSurface => {
            let (l, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(2.0 * (l * w + l * h + w * h))
        }
        AnnulusArea => {
            let (o, i) = (real(&args[0]), real(&args[1]));
            Value::Real(std::f64::consts::PI * (o * o - i * i))
        }
        SavingsFutureValue => {
            let (pmt, rate, n) = (real(&args[0]), real(&args[1]), int(&args[2]));
            if n < 0 {
                return Err("savings.future_value: the number of periods cannot be negative".to_string());
            }
            let r = rate / 100.0;
            let n = n as f64;
            Value::Real(if r == 0.0 { pmt * n } else { pmt * (((1.0 + r).powf(n) - 1.0) / r) })
        }
        DepreciationStraightLine => {
            let (cost, salvage, life) = (real(&args[0]), real(&args[1]), int(&args[2]));
            if life <= 0 {
                return Err("depreciation.straight_line: the life must be positive".to_string());
            }
            Value::Real((cost - salvage) / life as f64)
        }
        CentsToCents => to_int_checked((real(&args[0]) * 100.0).round())?,
        CentsFromCents => Value::Real(int(&args[0]) as f64 / 100.0),
        MarkupPrice => Value::Real(real(&args[0]) * (1.0 + real(&args[1]) / 100.0)),
        MarkupMargin => {
            let (price, cost) = (real(&args[0]), real(&args[1]));
            if price == 0.0 {
                return Err("markup.margin: the price is zero".to_string());
            }
            Value::Real((price - cost) / price * 100.0)
        }
        BmrMen => {
            let (w, h, age) = (real(&args[0]), real(&args[1]), int(&args[2]));
            Value::Real(88.362 + 13.397 * w + 4.799 * h - 5.677 * age as f64)
        }
        BmrWomen => {
            let (w, h, age) = (real(&args[0]), real(&args[1]), int(&args[2]));
            Value::Real(447.593 + 9.247 * w + 3.098 * h - 4.330 * age as f64)
        }
        WindChill => {
            let (t, v) = (real(&args[0]), real(&args[1]));
            let vp = v.powf(0.16);
            Value::Real(13.12 + 0.6215 * t - 11.37 * vp + 0.3965 * t * vp)
        }
        HeatIndex => Value::Real(heat_index(real(&args[0]), real(&args[1]))),
        DewpointOf => {
            let (t, rh) = (real(&args[0]), real(&args[1]));
            if rh <= 0.0 {
                return Err("dewpoint.of: the humidity must be positive".to_string());
            }
            let (a, b) = (17.27, 237.7);
            let alpha = (a * t) / (b + t) + (rh / 100.0).ln();
            Value::Real((b * alpha) / (a - alpha))
        }
        QuadraticDiscriminant => {
            let (a, b, c) = (real(&args[0]), real(&args[1]), real(&args[2]));
            Value::Real(b * b - 4.0 * a * c)
        }
        QuadraticRoot1 => Value::Real(quadratic_root(real(&args[0]), real(&args[1]), real(&args[2]), true)?),
        QuadraticRoot2 => Value::Real(quadratic_root(real(&args[0]), real(&args[1]), real(&args[2]), false)?),
        MotionSpeed => {
            let t = real(&args[1]);
            if t == 0.0 {
                return Err("motion.speed: the time is zero".to_string());
            }
            Value::Real(real(&args[0]) / t)
        }
        MotionTime => {
            let s = real(&args[1]);
            if s == 0.0 {
                return Err("motion.time: the speed is zero".to_string());
            }
            Value::Real(real(&args[0]) / s)
        }
        MotionDistance => Value::Real(real(&args[0]) * real(&args[1])),
        AprToMonthly => Value::Real(((1.0 + real(&args[0]) / 100.0).powf(1.0 / 12.0) - 1.0) * 100.0),
        AprToAnnual => Value::Real(((1.0 + real(&args[0]) / 100.0).powi(12) - 1.0) * 100.0),

        // module "grid": fast contiguous float arrays (the grid value is a Value::Floats)
        GridRange => grid_val((0..int(&args[0]).max(0)).map(|i| i as f64).collect()),
        GridZeros => grid_val(vec![0.0; int(&args[0]).max(0) as usize]),
        GridFill => grid_val(vec![real(&args[1]); int(&args[0]).max(0) as usize]),
        GridOf => grid_val(seq(&args[0]).borrow().iter().map(real).collect()),
        GridSize => Value::Int(floats(&args[0]).borrow().len() as i64),
        GridGet => {
            let i = int(&args[1]);
            let g = floats(&args[0]).borrow();
            if i < 1 || i as usize > g.len() {
                return Err(format!("nungw.get: there is no element number {}", i));
            }
            Value::Real(g[i as usize - 1])
        }
        GridSet => {
            let (i, val) = (int(&args[1]), real(&args[2]));
            let mut g = floats(&args[0]).borrow_mut();
            if i < 1 || i as usize > g.len() {
                return Err(format!("nungw.set: there is no element number {}", i));
            }
            g[i as usize - 1] = val;
            Value::Null
        }
        GridFree => Value::Null, // grids free themselves when no longer referenced
        GridToList => list(floats(&args[0]).borrow().iter().map(|&x| Value::Real(x)).collect()),
        GridAdd => grid_zip(&args[0], &args[1], |x, y| x + y)?,
        GridSub => grid_zip(&args[0], &args[1], |x, y| x - y)?,
        GridMul => grid_zip(&args[0], &args[1], |x, y| x * y)?,
        GridDiv => grid_zip(&args[0], &args[1], |x, y| x / y)?,
        GridScale => {
            let k = real(&args[1]);
            grid_map(&args[0], move |x| x * k)
        }
        GridOffset => {
            let k = real(&args[1]);
            grid_map(&args[0], move |x| x + k)
        }
        GridSubScalar => {
            let k = real(&args[1]);
            grid_map(&args[0], move |x| x - k)
        }
        GridScalarSub => {
            let k = real(&args[1]);
            grid_map(&args[0], move |x| k - x)
        }
        GridDivScalar => {
            let k = real(&args[1]);
            grid_map(&args[0], move |x| x / k)
        }
        GridScalarDiv => {
            let k = real(&args[1]);
            grid_map(&args[0], move |x| k / x)
        }
        GridSqrt => grid_map(&args[0], |x| x.sqrt()),
        GridSquare => grid_map(&args[0], |x| x * x),
        GridAbs => grid_map(&args[0], |x| x.abs()),
        GridSum => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, _) = &args[0] {
                return Ok(Value::Real(crate::gpu::full_reduce_resident(h, crate::gpu::ReduceOp::Sum).ok_or("nungw.sum: the GPU reduction failed")?));
            }
            Value::Real(reduce_sum(floats(&args[0]).borrow().as_slice()))
        }
        GridMean => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, shape) = &args[0] {
                if shape.iter().product::<usize>() == 0 {
                    return Err("nungw.mean: the nungw is empty".to_string());
                }
                return Ok(Value::Real(crate::gpu::full_reduce_resident(h, crate::gpu::ReduceOp::Mean).ok_or("nungw.mean: the GPU reduction failed")?));
            }
            let g = floats(&args[0]).borrow();
            if g.is_empty() {
                return Err("nungw.mean: the nungw is empty".to_string());
            }
            Value::Real(reduce_sum(g.as_slice()) / g.len() as f64)
        }
        #[cfg(feature = "native")]
        GridMax => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, _) = &args[0] {
                return Ok(Value::Real(crate::gpu::full_reduce_resident(h, crate::gpu::ReduceOp::Max).ok_or("nungw.max: the GPU reduction failed")?));
            }
            Value::Real(floats(&args[0]).borrow().par_iter().cloned().reduce(|| f64::NEG_INFINITY, f64::max))
        }
        #[cfg(not(feature = "native"))]
        GridMax => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, _) = &args[0] {
                return Ok(Value::Real(crate::gpu::full_reduce_resident(h, crate::gpu::ReduceOp::Max).ok_or("nungw.max: the GPU reduction failed")?));
            }
            Value::Real(floats(&args[0]).borrow().iter().cloned().fold(f64::NEG_INFINITY, f64::max))
        }
        #[cfg(feature = "native")]
        GridMin => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, _) = &args[0] {
                return Ok(Value::Real(crate::gpu::full_reduce_resident(h, crate::gpu::ReduceOp::Min).ok_or("nungw.min: the GPU reduction failed")?));
            }
            Value::Real(floats(&args[0]).borrow().par_iter().cloned().reduce(|| f64::INFINITY, f64::min))
        }
        #[cfg(not(feature = "native"))]
        GridMin => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, _) = &args[0] {
                return Ok(Value::Real(crate::gpu::full_reduce_resident(h, crate::gpu::ReduceOp::Min).ok_or("nungw.min: the GPU reduction failed")?));
            }
            Value::Real(floats(&args[0]).borrow().iter().cloned().fold(f64::INFINITY, f64::min))
        }
        GridDot => {
            let (va, vb) = (floats(&args[0]).borrow(), floats(&args[1]).borrow());
            if va.len() != vb.len() {
                return Err(format!("nungw.dot: the sizes differ ({} and {})", va.len(), vb.len()));
            }
            Value::Real(reduce_dot(va.as_slice(), vb.as_slice()))
        }
        GridSumSquares => Value::Real(reduce_sumsq(floats(&args[0]).borrow().as_slice())),
        GridNorm => Value::Real(reduce_sumsq(floats(&args[0]).borrow().as_slice()).sqrt()),
        GridExp => grid_map(&args[0], |x| x.exp()),
        GridExp2 => grid_map(&args[0], |x| x.exp2()),
        GridExpm1 => grid_map(&args[0], |x| x.exp_m1()),
        GridLog => grid_map(&args[0], |x| x.ln()),
        GridLog2 => grid_map(&args[0], |x| x.log2()),
        GridLog10 => grid_map(&args[0], |x| x.log10()),
        GridLog1p => grid_map(&args[0], |x| x.ln_1p()),
        GridSin => grid_map(&args[0], |x| x.sin()),
        GridCos => grid_map(&args[0], |x| x.cos()),
        GridTan => grid_map(&args[0], |x| x.tan()),
        GridAsin => grid_map(&args[0], |x| x.asin()),
        GridAcos => grid_map(&args[0], |x| x.acos()),
        GridAtan => grid_map(&args[0], |x| x.atan()),
        GridSinh => grid_map(&args[0], |x| x.sinh()),
        GridCosh => grid_map(&args[0], |x| x.cosh()),
        GridTanh => grid_map(&args[0], |x| x.tanh()),
        GridAsinh => grid_map(&args[0], |x| x.asinh()),
        GridAcosh => grid_map(&args[0], |x| x.acosh()),
        GridAtanh => grid_map(&args[0], |x| x.atanh()),
        GridFloor => grid_map(&args[0], |x| x.floor()),
        GridCeil => grid_map(&args[0], |x| x.ceil()),
        GridRound => grid_map(&args[0], |x| x.round_ties_even()),
        GridTrunc => grid_map(&args[0], |x| x.trunc()),
        GridSign => grid_map(&args[0], |x| if x.is_nan() { x } else if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 }),
        GridCbrt => grid_map(&args[0], |x| x.cbrt()),
        GridRecip => grid_map(&args[0], |x| x.recip()),
        GridNeg => grid_map(&args[0], |x| -x),
        GridDegrees => grid_map(&args[0], |x| x.to_degrees()),
        GridRadians => grid_map(&args[0], |x| x.to_radians()),
        GridPow => grid_zip(&args[0], &args[1], |x, y| x.powf(y))?,
        GridAtan2 => grid_zip(&args[0], &args[1], |x, y| x.atan2(y))?,
        GridHypot => grid_zip(&args[0], &args[1], |x, y| x.hypot(y))?,
        GridMod => grid_zip(&args[0], &args[1], |x, y| x - (x / y).floor() * y)?,
        GridMaximum => grid_zip(&args[0], &args[1], |x, y| x.max(y))?,
        GridMinimum => grid_zip(&args[0], &args[1], |x, y| x.min(y))?,
        GridProd => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, _) = &args[0] {
                return Ok(Value::Real(crate::gpu::full_reduce_resident(h, crate::gpu::ReduceOp::Prod).ok_or("nungw.prod: the GPU reduction failed")?));
            }
            Value::Real(reduce_prod(floats(&args[0]).borrow().as_slice()))
        }
        GridVar => {
            let g = floats(&args[0]).borrow();
            if g.is_empty() {
                return Err("nungw.var: the nungw is empty".to_string());
            }
            let n = g.len() as f64;
            let mean = reduce_sum(g.as_slice()) / n;
            Value::Real(reduce_sumsqdev(g.as_slice(), mean) / n)
        }
        GridStd => {
            let g = floats(&args[0]).borrow();
            if g.is_empty() {
                return Err("nungw.std: the nungw is empty".to_string());
            }
            let n = g.len() as f64;
            let mean = reduce_sum(g.as_slice()) / n;
            let var = reduce_sumsqdev(g.as_slice(), mean) / n;
            Value::Real(var.sqrt())
        }
        GridMedian => {
            let g = floats(&args[0]).borrow();
            if g.is_empty() {
                return Err("nungw.median: the nungw is empty".to_string());
            }
            let mut v: Vec<f64> = g.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let m = v.len();
            Value::Real(if m % 2 == 1 { v[m / 2] } else { (v[m / 2 - 1] + v[m / 2]) / 2.0 })
        }
        GridArgmax => {
            let g = floats(&args[0]).borrow();
            if g.is_empty() {
                return Err("nungw.argmax: the nungw is empty".to_string());
            }
            let (mut bi, mut bv) = (0usize, g[0]);
            for (i, &x) in g.iter().enumerate() {
                if x > bv {
                    bv = x;
                    bi = i;
                }
            }
            Value::Real((bi + 1) as f64)
        }
        GridArgmin => {
            let g = floats(&args[0]).borrow();
            if g.is_empty() {
                return Err("nungw.argmin: the nungw is empty".to_string());
            }
            let (mut bi, mut bv) = (0usize, g[0]);
            for (i, &x) in g.iter().enumerate() {
                if x < bv {
                    bv = x;
                    bi = i;
                }
            }
            Value::Real((bi + 1) as f64)
        }
        GridPtp => {
            let g = floats(&args[0]).borrow();
            if g.is_empty() {
                return Err("nungw.ptp: the nungw is empty".to_string());
            }
            let mx = g.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mn = g.iter().cloned().fold(f64::INFINITY, f64::min);
            Value::Real(mx - mn)
        }
        GridFft => {
            let v = floats(&args[0]).borrow();
            let n = v.len();
            if n == 0 {
                return Err("nungw.fft: the nungw is empty".to_string());
            }
            let mut buf: Vec<Complex<f64>> = v.iter().map(|&x| Complex::new(x, 0.0)).collect();
            FftPlanner::new().plan_fft_forward(n).process(&mut buf);
            let mut out = Vec::with_capacity(2 * n);
            for c in &buf {
                out.push(c.re);
                out.push(c.im);
            }
            grid_val(out)
        }
        GridIfft => {
            let v = floats(&args[0]).borrow();
            if v.len() % 2 != 0 {
                return Err("nungw.ifft: a complex nungw has an even length (re, im, re, im, ...)".to_string());
            }
            let n = v.len() / 2;
            if n == 0 {
                return Err("nungw.ifft: the nungw is empty".to_string());
            }
            let mut buf: Vec<Complex<f64>> = (0..n).map(|i| Complex::new(v[2 * i], v[2 * i + 1])).collect();
            FftPlanner::new().plan_fft_inverse(n).process(&mut buf);
            let scale = 1.0 / n as f64;
            let mut out = Vec::with_capacity(2 * n);
            for c in &buf {
                out.push(c.re * scale);
                out.push(c.im * scale);
            }
            grid_val(out)
        }
        GridCabs => {
            let v = floats(&args[0]).borrow();
            if v.len() % 2 != 0 {
                return Err("nungw.cabs: a complex nungw has an even length (re, im, re, im, ...)".to_string());
            }
            let out: Vec<f64> = (0..v.len() / 2).map(|i| v[2 * i].hypot(v[2 * i + 1])).collect();
            grid_val(out)
        }
        GridDet => {
            let n = if args.len() == 1 {
                let s = shape_of(&args[0]);
                if s.len() != 2 || s[0] != s[1] {
                    return Err("nungw.det: the argument must be a square 2-D nungw (or pass n)".to_string());
                }
                s[0]
            } else {
                let n = int(&args[1]);
                if n < 0 {
                    return Err("nungw.det: the dimension cannot be negative".to_string());
                }
                n as usize
            };
            let v = floats(&args[0]).borrow();
            if v.len() != n * n {
                return Err(format!("nungw.det: a {n}x{n} matrix needs {} values, it has {}", n * n, v.len()));
            }
            if n == 0 {
                Value::Real(1.0)
            } else {
                let mut m = v.to_vec();
                let mut perm = vec![0usize; n];
                match lu_in_place(&mut m, n, &mut perm) {
                    None => Value::Real(0.0),
                    Some(sign) => {
                        let mut d = sign;
                        for i in 0..n {
                            d *= m[i * n + i];
                        }
                        Value::Real(d)
                    }
                }
            }
        }
        GridSolve => {
            let (n, b_idx) = if args.len() == 2 {
                let s = shape_of(&args[0]);
                if s.len() != 2 || s[0] != s[1] {
                    return Err("nungw.solve: the matrix must be square 2-D (or pass n)".to_string());
                }
                (s[0], 1usize)
            } else {
                let n = int(&args[1]);
                if n < 0 {
                    return Err("nungw.solve: the dimension cannot be negative".to_string());
                }
                (n as usize, 2usize)
            };
            let a0 = floats(&args[0]).borrow();
            let b0 = floats(&args[b_idx]).borrow();
            if a0.len() != n * n {
                return Err(format!("nungw.solve: a {n}x{n} matrix needs {} values, it has {}", n * n, a0.len()));
            }
            if b0.len() != n {
                return Err(format!("nungw.solve: the right-hand side needs {n} values, it has {}", b0.len()));
            }
            let mut m = a0.to_vec();
            let mut perm = vec![0usize; n];
            match lu_in_place(&mut m, n, &mut perm) {
                None => return Err("nungw.solve: the matrix is singular".to_string()),
                Some(_) => grid_val(lu_solve(&m, n, &perm, &b0)),
            }
        }
        GridInv => {
            let n = if args.len() == 1 {
                let s = shape_of(&args[0]);
                if s.len() != 2 || s[0] != s[1] {
                    return Err("nungw.inv: the argument must be a square 2-D nungw (or pass n)".to_string());
                }
                s[0]
            } else {
                let n = int(&args[1]);
                if n < 0 {
                    return Err("nungw.inv: the dimension cannot be negative".to_string());
                }
                n as usize
            };
            let a0 = floats(&args[0]).borrow();
            if a0.len() != n * n {
                return Err(format!("nungw.inv: a {n}x{n} matrix needs {} values, it has {}", n * n, a0.len()));
            }
            let mut m = a0.to_vec();
            let mut perm = vec![0usize; n];
            match lu_in_place(&mut m, n, &mut perm) {
                None => return Err("nungw.inv: the matrix is singular".to_string()),
                Some(_) => {
                    let mut out = vec![0.0; n * n];
                    let mut e = vec![0.0; n];
                    for col in 0..n {
                        for (i, slot) in e.iter_mut().enumerate() {
                            *slot = if i == col { 1.0 } else { 0.0 };
                        }
                        let x = lu_solve(&m, n, &perm, &e);
                        for i in 0..n {
                            out[i * n + col] = x[i];
                        }
                    }
                    grid_val_shaped(out, vec![n, n])
                }
            }
        }
        GridTranspose => {
            // shape-aware form: transpose(a) using a's 2-D shape.
            if args.len() == 1 {
                let s = shape_of(&args[0]);
                if s.len() != 2 {
                    return Err("nungw.transpose: the argument must be a 2-D nungw (or pass rows and cols)".to_string());
                }
                let (rows, cols) = (s[0], s[1]);
                let v = floats(&args[0]).borrow();
                let mut out = vec![0.0; rows * cols];
                for r in 0..rows {
                    for c in 0..cols {
                        out[c * rows + r] = v[r * cols + c];
                    }
                }
                return Ok(grid_val_shaped(out, vec![cols, rows]));
            }
            let (rows, cols) = (int(&args[1]), int(&args[2]));
            if rows < 0 || cols < 0 {
                return Err("nungw.transpose: the dimensions cannot be negative".to_string());
            }
            let (rows, cols) = (rows as usize, cols as usize);
            let v = floats(&args[0]).borrow();
            if v.len() != rows * cols {
                return Err(format!("nungw.transpose: a {rows}x{cols} matrix needs {} values, it has {}", rows * cols, v.len()));
            }
            let mut out = vec![0.0; rows * cols];
            for r in 0..rows {
                for c in 0..cols {
                    out[c * rows + r] = v[r * cols + c];
                }
            }
            grid_val_shaped(out, vec![cols, rows])
        }
        GridShape => {
            let shape = shape_of(&args[0]);
            Value::Seq(Rc::new(RefCell::new(shape.iter().map(|&d| Value::Int(d as i64)).collect())))
        }
        GridNdim => Value::Int(shape_of(&args[0]).len() as i64),
        GridReshape => {
            let dims: Vec<usize> = seq(&args[1])
                .borrow()
                .iter()
                .map(|v| match v {
                    Value::Int(n) => (*n).max(0) as usize,
                    Value::Real(r) => r.max(0.0) as usize,
                    _ => 0,
                })
                .collect();
            let total: usize = dims.iter().product();
            // A resident array reshapes by metadata only: same VRAM buffer (shared
            // Rc), new shape -- so a reshape stays on the GPU (needed for keepdims).
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, shape) = &args[0] {
                let len: usize = shape.iter().product();
                if total != len {
                    return Err(format!("nungw.reshape: {len} values cannot form a shape with {total} cells"));
                }
                return Ok(Value::GpuFloats(Rc::clone(h), Rc::new(dims)));
            }
            let data = floats(&args[0]);
            let len = data.borrow().len();
            if total != len {
                return Err(format!("nungw.reshape: {len} values cannot form a shape with {total} cells"));
            }
            Value::Floats(data.clone(), Rc::new(dims))
        }
        GridSumAxis => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, shape) = &args[0] {
                return gpu_axis_reduce(h, shape, int(&args[1]), crate::gpu::ReduceOp::Sum, "sum_axis");
            }
            axis_reduce(&args[0], int(&args[1]), 0.0, |acc, x| acc + x, |v, _| v)?
        }
        GridProdAxis => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, shape) = &args[0] {
                return gpu_axis_reduce(h, shape, int(&args[1]), crate::gpu::ReduceOp::Prod, "prod_axis");
            }
            axis_reduce(&args[0], int(&args[1]), 1.0, |acc, x| acc * x, |v, _| v)?
        }
        GridMeanAxis => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, shape) = &args[0] {
                return gpu_axis_reduce(h, shape, int(&args[1]), crate::gpu::ReduceOp::Mean, "mean_axis");
            }
            axis_reduce(&args[0], int(&args[1]), 0.0, |acc, x| acc + x, |v, m| v / m as f64)?
        }
        GridMaxAxis => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, shape) = &args[0] {
                return gpu_axis_reduce(h, shape, int(&args[1]), crate::gpu::ReduceOp::Max, "max_axis");
            }
            axis_reduce(&args[0], int(&args[1]), f64::NEG_INFINITY, f64::max, |v, _| v)?
        }
        GridMinAxis => {
            #[cfg(feature = "gpu")]
            if let Value::GpuFloats(h, shape) = &args[0] {
                return gpu_axis_reduce(h, shape, int(&args[1]), crate::gpu::ReduceOp::Min, "min_axis");
            }
            axis_reduce(&args[0], int(&args[1]), f64::INFINITY, f64::min, |v, _| v)?
        }
        GridAt => {
            let shape = shape_of(&args[0]);
            let idx = &args[1..];
            if idx.len() != shape.len() {
                return Err(format!("nungw.at: the nungw is {}-D but got {} indices", shape.len(), idx.len()));
            }
            let mut strides = vec![1usize; shape.len()];
            for d in (0..shape.len()).rev().skip(1) {
                strides[d] = strides[d + 1] * shape[d + 1];
            }
            let mut off = 0usize;
            for d in 0..shape.len() {
                let i = int(&idx[d]);
                if i < 1 || i as usize > shape[d] {
                    return Err(format!("nungw.at: index {i} is out of range 1..{} on dimension {}", shape[d], d + 1));
                }
                off += (i as usize - 1) * strides[d];
            }
            Value::Real(floats(&args[0]).borrow()[off])
        }
        GridRand => {
            let n = int(&args[0]).max(0) as usize;
            grid_val((0..n).map(|_| (rng_u64() >> 11) as f64 / (1u64 << 53) as f64).collect())
        }
        GridRandn => {
            let n = int(&args[0]).max(0) as usize;
            let unif = || ((rng_u64() >> 11) as f64 / (1u64 << 53) as f64).max(1e-300);
            let mut out = Vec::with_capacity(n);
            let mut i = 0;
            while i < n {
                // Box-Muller: two independent standard normals per pair of uniforms
                let r = (-2.0 * unif().ln()).sqrt();
                let (s, c) = (std::f64::consts::TAU * unif()).sin_cos();
                out.push(r * c);
                if i + 1 < n {
                    out.push(r * s);
                }
                i += 2;
            }
            grid_val(out)
        }
        GridEye => {
            let n = int(&args[0]).max(0) as usize;
            let mut out = vec![0.0; n * n];
            for i in 0..n {
                out[i * n + i] = 1.0;
            }
            grid_val_shaped(out, vec![n, n])
        }
        GridCumsum => {
            let v = floats(&args[0]).borrow();
            let mut out = Vec::with_capacity(v.len());
            let mut acc = 0.0;
            for &x in v.iter() {
                acc += x;
                out.push(acc);
            }
            grid_val(out)
        }
        GridCumprod => {
            let v = floats(&args[0]).borrow();
            let mut out = Vec::with_capacity(v.len());
            let mut acc = 1.0;
            for &x in v.iter() {
                acc *= x;
                out.push(acc);
            }
            grid_val(out)
        }
        GridSort => {
            let mut out = floats(&args[0]).borrow().to_vec();
            out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            grid_val(out)
        }
        GridArgsort => {
            let v = floats(&args[0]).borrow();
            let mut idx: Vec<usize> = (0..v.len()).collect();
            idx.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap_or(Ordering::Equal));
            grid_val(idx.iter().map(|&i| (i + 1) as f64).collect())
        }
        GridClip => {
            let lo = real(&args[1]);
            let hi = real(&args[2]);
            grid_map(&args[0], move |x| x.max(lo).min(hi))
        }
        GridConcat => {
            let mut out = floats(&args[0]).borrow().to_vec();
            out.extend_from_slice(&floats(&args[1]).borrow());
            grid_val(out)
        }
        GridWhere => {
            let (c, a1, b1) =
                (floats(&args[0]).borrow(), floats(&args[1]).borrow(), floats(&args[2]).borrow());
            if c.len() != a1.len() || c.len() != b1.len() {
                return Err(format!("nungw.where: the sizes differ ({}, {}, {})", c.len(), a1.len(), b1.len()));
            }
            let data: Vec<f64> = (0..c.len()).map(|i| if c[i] != 0.0 { a1[i] } else { b1[i] }).collect();
            grid_val_shaped(data, (*shape_of(&args[0])).clone())
        }
        GridConcatAxis => {
            let axis = int(&args[2]);
            let (sa, sb) = (shape_of(&args[0]), shape_of(&args[1]));
            if sa.len() != sb.len() {
                return Err("nungw.concat_axis: the arrays have different numbers of dimensions".to_string());
            }
            if axis < 0 || axis as usize >= sa.len() {
                return Err(format!("nungw.concat_axis: axis {axis} is out of range for a {}-D nungw", sa.len()));
            }
            let axis = axis as usize;
            for d in 0..sa.len() {
                if d != axis && sa[d] != sb[d] {
                    return Err(format!("nungw.concat_axis: shapes {:?} and {:?} differ away from axis {axis}", *sa, *sb));
                }
            }
            let inner: usize = sa[axis + 1..].iter().product();
            let outer: usize = sa[..axis].iter().product();
            let (va, vb) = (floats(&args[0]).borrow(), floats(&args[1]).borrow());
            let (ba, bb) = (sa[axis] * inner, sb[axis] * inner);
            let mut out = Vec::with_capacity(va.len() + vb.len());
            for o in 0..outer {
                out.extend_from_slice(&va[o * ba..(o + 1) * ba]);
                out.extend_from_slice(&vb[o * bb..(o + 1) * bb]);
            }
            let mut out_shape = (*sa).clone();
            out_shape[axis] = sa[axis] + sb[axis];
            grid_val_shaped(out, out_shape)
        }
        GridSlice => {
            let (start, stop) = (int(&args[1]), int(&args[2]));
            let v = floats(&args[0]).borrow();
            let len = v.len() as i64;
            if start < 1 || stop > len || start > stop {
                return Err(format!("nungw.slice: range {start}..{stop} is invalid for {len} values (1-based, inclusive)"));
            }
            grid_val(v[(start as usize - 1)..(stop as usize)].to_vec())
        }
        GridRows => {
            // a range of rows of a 2-D matrix, 1-based inclusive, keeping the shape.
            let shape = shape_of(&args[0]);
            if shape.len() != 2 {
                return Err("nungw.rows: the argument must be a 2-D nungw".to_string());
            }
            let (m, n) = (shape[0] as i64, shape[1]);
            let (start, stop) = (int(&args[1]), int(&args[2]));
            if start < 1 || stop > m || start > stop {
                return Err(format!("nungw.rows: rows {start}..{stop} are invalid for a matrix with {m} rows (1-based, inclusive)"));
            }
            let v = floats(&args[0]).borrow();
            let (lo, hi) = ((start as usize - 1) * n, stop as usize * n);
            grid_val_shaped(v[lo..hi].to_vec(), vec![(stop - start + 1) as usize, n])
        }
        GridEigvalsSym => {
            let n = if args.len() == 1 {
                let s = shape_of(&args[0]);
                if s.len() != 2 || s[0] != s[1] {
                    return Err("nungw.eigvals_sym: the argument must be a square 2-D nungw (or pass n)".to_string());
                }
                s[0]
            } else {
                let n = int(&args[1]);
                if n < 0 {
                    return Err("nungw.eigvals_sym: the dimension cannot be negative".to_string());
                }
                n as usize
            };
            let v = floats(&args[0]).borrow();
            if v.len() != n * n {
                return Err(format!("nungw.eigvals_sym: a {n}x{n} matrix needs {} values, it has {}", n * n, v.len()));
            }
            let mut vals = jacobi_eigenvalues(&v[..], n);
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            grid_val(vals)
        }
        GridSingularValues => {
            let (m, n) = if args.len() == 1 {
                let s = shape_of(&args[0]);
                if s.len() != 2 {
                    return Err("nungw.singular_values: the argument must be a 2-D nungw (or pass m and n)".to_string());
                }
                (s[0], s[1])
            } else {
                let (m, n) = (int(&args[1]), int(&args[2]));
                if m < 0 || n < 0 {
                    return Err("nungw.singular_values: the dimensions cannot be negative".to_string());
                }
                (m as usize, n as usize)
            };
            let v = floats(&args[0]).borrow();
            if v.len() != m * n {
                return Err(format!("nungw.singular_values: a {m}x{n} matrix needs {} values, it has {}", m * n, v.len()));
            }
            // singular values of A = sqrt of the eigenvalues of A^T A (n x n, symmetric)
            let mut ata = vec![0.0; n * n];
            for i in 0..n {
                for j in 0..n {
                    let mut s = 0.0;
                    for k in 0..m {
                        s += v[k * n + i] * v[k * n + j];
                    }
                    ata[i * n + j] = s;
                }
            }
            let mut sv: Vec<f64> = jacobi_eigenvalues(&ata, n).iter().map(|&x| x.max(0.0).sqrt()).collect();
            sv.sort_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));
            sv.truncate(m.min(n));
            grid_val(sv)
        }
        GridEigvecsSym => {
            let n = if args.len() == 1 {
                let s = shape_of(&args[0]);
                if s.len() != 2 || s[0] != s[1] {
                    return Err("nungw.eigvecs_sym: the argument must be a square 2-D nungw (or pass n)".to_string());
                }
                s[0]
            } else {
                let n = int(&args[1]);
                if n < 0 {
                    return Err("nungw.eigvecs_sym: the dimension cannot be negative".to_string());
                }
                n as usize
            };
            let v = floats(&args[0]).borrow();
            if v.len() != n * n {
                return Err(format!("nungw.eigvecs_sym: a {n}x{n} matrix needs {} values, it has {}", n * n, v.len()));
            }
            let (vals, vecs) = jacobi_eigen(&v[..], n);
            // columns ordered to match eigvals_sym (eigenvalues ascending)
            let mut idx: Vec<usize> = (0..n).collect();
            idx.sort_by(|&i, &j| vals[i].partial_cmp(&vals[j]).unwrap_or(Ordering::Equal));
            let mut out = vec![0.0; n * n];
            for (newc, &oldc) in idx.iter().enumerate() {
                for row in 0..n {
                    out[row * n + newc] = vecs[row * n + oldc];
                }
            }
            grid_val_shaped(out, vec![n, n])
        }
        GridSvdU | GridSvdV => {
            let (m, n) = if args.len() == 1 {
                let s = shape_of(&args[0]);
                if s.len() != 2 {
                    return Err("nungw.svd: the argument must be a 2-D nungw (or pass m and n)".to_string());
                }
                (s[0], s[1])
            } else {
                let (m, n) = (int(&args[1]), int(&args[2]));
                if m < 0 || n < 0 {
                    return Err("nungw.svd: the dimensions cannot be negative".to_string());
                }
                (m as usize, n as usize)
            };
            let v = floats(&args[0]).borrow();
            if v.len() != m * n {
                return Err(format!("nungw.svd: a {m}x{n} matrix needs {} values, it has {}", m * n, v.len()));
            }
            let (_sig, umat, vmat) = svd_thin(&v[..], m, n);
            let r = m.min(n);
            if b == GridSvdU {
                grid_val_shaped(umat, vec![m, r])
            } else {
                grid_val_shaped(vmat, vec![n, r])
            }
        }
        GridLstsq => {
            let (m, n, b_idx) = if args.len() == 2 {
                let s = shape_of(&args[0]);
                if s.len() != 2 {
                    return Err("nungw.lstsq: the matrix must be a 2-D nungw (or pass m and n)".to_string());
                }
                (s[0], s[1], 1usize)
            } else {
                let (m, n) = (int(&args[1]), int(&args[2]));
                if m < 0 || n < 0 {
                    return Err("nungw.lstsq: the dimensions cannot be negative".to_string());
                }
                (m as usize, n as usize, 3usize)
            };
            let a0 = floats(&args[0]).borrow();
            let b0 = floats(&args[b_idx]).borrow();
            if a0.len() != m * n {
                return Err(format!("nungw.lstsq: a {m}x{n} matrix needs {} values, it has {}", m * n, a0.len()));
            }
            if b0.len() != m {
                return Err(format!("nungw.lstsq: the right-hand side needs {m} values, it has {}", b0.len()));
            }
            // normal equations: (A^T A) x = A^T b
            let mut ata = vec![0.0; n * n];
            let mut atb = vec![0.0; n];
            for i in 0..n {
                for k in 0..m {
                    atb[i] += a0[k * n + i] * b0[k];
                }
                for j in 0..n {
                    let mut s = 0.0;
                    for k in 0..m {
                        s += a0[k * n + i] * a0[k * n + j];
                    }
                    ata[i * n + j] = s;
                }
            }
            let mut perm = vec![0usize; n];
            match lu_in_place(&mut ata, n, &mut perm) {
                None => return Err("nungw.lstsq: the system is rank-deficient".to_string()),
                Some(_) => grid_val(lu_solve(&ata, n, &perm, &atb)),
            }
        }
        GridEval => grid_eval(&args[0], &args[1..])?,
        GridGpu => {
            let on = matches!(&args[0], Value::Bool(true));
            GPU_REQUESTED.store(on, std::sync::atomic::Ordering::Relaxed);
            // report whether the GPU is actually usable now (feature on + adapter)
            #[cfg(feature = "gpu")]
            let active = on && crate::gpu::available();
            #[cfg(not(feature = "gpu"))]
            let active = false;
            Value::Bool(active)
        }
        #[cfg(feature = "gpu")]
        GridToGpu => match &args[0] {
            Value::GpuFloats(..) => args[0].clone(), // already resident
            _ => {
                let f32v: Vec<f32> = floats(&args[0]).borrow().iter().map(|&x| x as f32).collect();
                match crate::gpu::upload(&f32v) {
                    Some(h) => Value::GpuFloats(h, shape_of(&args[0])),
                    None => return Err("nungw.to_gpu: no GPU is available".to_string()),
                }
            }
        },
        #[cfg(not(feature = "gpu"))]
        GridToGpu => args[0].clone(), // no GPU in this build: a no-op
        #[cfg(feature = "gpu")]
        GridToCpu => match &args[0] {
            Value::GpuFloats(h, shape) => {
                let f32v = crate::gpu::download(h).ok_or("nungw.to_cpu: the download failed")?;
                let f64v: Vec<f64> = f32v.iter().map(|&x| x as f64).collect();
                grid_val_shaped(f64v, (**shape).clone())
            }
            _ => args[0].clone(), // already on the CPU
        },
        #[cfg(not(feature = "gpu"))]
        GridToCpu => args[0].clone(),
        GridMatmul => {
            // resident path: operands already on the GPU -> compute on-device,
            // result stays resident (no upload/download).
            #[cfg(feature = "gpu")]
            if let Some(out) = try_matmul_resident(args)? {
                return Ok(out);
            }
            // shape-aware form: matmul(a, b) with dimensions from the 2-D shapes.
            if args.len() == 2 {
                let (sa, sb) = (shape_of(&args[0]), shape_of(&args[1]));
                if sa.len() != 2 || sb.len() != 2 {
                    return Err("nungw.matmul: both arguments must be 2-D nungws (or pass dimensions explicitly)".to_string());
                }
                let (rows, inner, cols) = (sa[0], sa[1], sb[1]);
                if sb[0] != inner {
                    return Err(format!("nungw.matmul: inner dimensions do not match ({}x{} times {}x{})", rows, inner, sb[0], cols));
                }
                let va = floats(&args[0]).borrow();
                let vb = floats(&args[1]).borrow();
                #[cfg(feature = "gpu")]
                if let Some(out) = try_gpu_matmul(va.as_slice(), vb.as_slice(), rows, inner, cols) {
                    return Ok(grid_val_shaped(out, vec![rows, cols]));
                }
                let mut out: Vec<f64> = Vec::with_capacity(rows * cols);
                // SAFETY: matmul_into writes all rows*cols elements before they are read.
                unsafe { out.set_len(rows * cols) };
                matmul_into(&mut out, &va, &vb, rows, inner, cols);
                return Ok(grid_val_shaped(out, vec![rows, cols]));
            }
            // a is (rows x inner), b is (inner x cols), both row-major; result rows x cols
            let (rows, inner, cols) = (int(&args[1]), int(&args[2]), int(&args[4]));
            if rows < 0 || inner < 0 || cols < 0 {
                return Err("nungw.matmul: the dimensions cannot be negative".to_string());
            }
            let (rows, inner, cols) = (rows as usize, inner as usize, cols as usize);
            let va = floats(&args[0]).borrow();
            let vb = floats(&args[3]).borrow();
            if va.len() != rows * inner {
                return Err(format!("nungw.matmul: the first matrix should have {} values ({}x{}), it has {}", rows * inner, rows, inner, va.len()));
            }
            if vb.len() != inner * cols {
                return Err(format!("nungw.matmul: the second matrix should have {} values ({}x{}), it has {}", inner * cols, inner, cols, vb.len()));
            }
            #[cfg(feature = "gpu")]
            if let Some(out) = try_gpu_matmul(va.as_slice(), vb.as_slice(), rows, inner, cols) {
                return Ok(grid_val_shaped(out, vec![rows, cols]));
            }
            // the kernel overwrites every element (beta = 0), so skip zero-filling
            let mut out: Vec<f64> = Vec::with_capacity(rows * cols);
            // SAFETY: matmul_into writes all rows*cols elements before they are read.
            unsafe { out.set_len(rows * cols) };
            matmul_into(&mut out, &va, &vb, rows, inner, cols);
            grid_val_shaped(out, vec![rows, cols])
        }
        GridMatmulTA => {
            #[cfg(feature = "gpu")]
            if let Some(out) = try_matmul_ta_tb_resident(args, true, false)? {
                return Ok(out);
            }
            // shape-aware form: matmul_ta(a, b) with dimensions from the 2-D shapes.
            if args.len() == 2 {
                let (sa, sb) = (shape_of(&args[0]), shape_of(&args[1]));
                if sa.len() != 2 || sb.len() != 2 {
                    return Err("nungw.matmul_ta: both arguments must be 2-D nungws (or pass dimensions explicitly)".to_string());
                }
                let (a_rows, a_cols, b_cols) = (sa[0], sa[1], sb[1]);
                if sb[0] != a_rows {
                    return Err(format!("nungw.matmul_ta: the second matrix must have {} rows to match the first, it has {}", a_rows, sb[0]));
                }
                let va = floats(&args[0]).borrow();
                let vb = floats(&args[1]).borrow();
                let mut out: Vec<f64> = Vec::with_capacity(a_cols * b_cols);
                // SAFETY: matmul_trans writes all a_cols*b_cols elements before they are read.
                unsafe { out.set_len(a_cols * b_cols) };
                matmul_trans(&mut out, &va, &vb, a_cols, a_rows, b_cols, true, false);
                return Ok(grid_val_shaped(out, vec![a_cols, b_cols]));
            }
            // A^T * B, with A stored (a_rows x a_cols); B is (a_rows x b_cols);
            // result is (a_cols x b_cols). No explicit transpose of A.
            let (a_rows, a_cols, b_cols) = (int(&args[1]), int(&args[2]), int(&args[4]));
            if a_rows < 0 || a_cols < 0 || b_cols < 0 {
                return Err("nungw.matmul_ta: the dimensions cannot be negative".to_string());
            }
            let (a_rows, a_cols, b_cols) = (a_rows as usize, a_cols as usize, b_cols as usize);
            let va = floats(&args[0]).borrow();
            let vb = floats(&args[3]).borrow();
            if va.len() != a_rows * a_cols {
                return Err(format!("nungw.matmul_ta: the first matrix should have {} values ({}x{}), it has {}", a_rows * a_cols, a_rows, a_cols, va.len()));
            }
            if vb.len() != a_rows * b_cols {
                return Err(format!("nungw.matmul_ta: the second matrix should have {} values ({}x{}), it has {}", a_rows * b_cols, a_rows, b_cols, vb.len()));
            }
            let mut out: Vec<f64> = Vec::with_capacity(a_cols * b_cols);
            // SAFETY: matmul_trans writes all a_cols*b_cols elements before they are read.
            unsafe { out.set_len(a_cols * b_cols) };
            matmul_trans(&mut out, &va, &vb, a_cols, a_rows, b_cols, true, false);
            grid_val_shaped(out, vec![a_cols, b_cols])
        }
        GridMatmulTB => {
            #[cfg(feature = "gpu")]
            if let Some(out) = try_matmul_ta_tb_resident(args, false, true)? {
                return Ok(out);
            }
            // shape-aware form: matmul_tb(a, b) with dimensions from the 2-D shapes.
            if args.len() == 2 {
                let (sa, sb) = (shape_of(&args[0]), shape_of(&args[1]));
                if sa.len() != 2 || sb.len() != 2 {
                    return Err("nungw.matmul_tb: both arguments must be 2-D nungws (or pass dimensions explicitly)".to_string());
                }
                let (a_rows, inner, b_rows) = (sa[0], sa[1], sb[0]);
                if sb[1] != inner {
                    return Err(format!("nungw.matmul_tb: the second matrix must have {} columns to match the first, it has {}", inner, sb[1]));
                }
                let va = floats(&args[0]).borrow();
                let vb = floats(&args[1]).borrow();
                let mut out: Vec<f64> = Vec::with_capacity(a_rows * b_rows);
                // SAFETY: matmul_trans writes all a_rows*b_rows elements before they are read.
                unsafe { out.set_len(a_rows * b_rows) };
                matmul_trans(&mut out, &va, &vb, a_rows, inner, b_rows, false, true);
                return Ok(grid_val_shaped(out, vec![a_rows, b_rows]));
            }
            // A * B^T, with A (a_rows x inner) and B stored (b_rows x inner);
            // result is (a_rows x b_rows). No explicit transpose of B.
            let (a_rows, inner, b_rows) = (int(&args[1]), int(&args[2]), int(&args[4]));
            if a_rows < 0 || inner < 0 || b_rows < 0 {
                return Err("nungw.matmul_tb: the dimensions cannot be negative".to_string());
            }
            let (a_rows, inner, b_rows) = (a_rows as usize, inner as usize, b_rows as usize);
            let va = floats(&args[0]).borrow();
            let vb = floats(&args[3]).borrow();
            if va.len() != a_rows * inner {
                return Err(format!("nungw.matmul_tb: the first matrix should have {} values ({}x{}), it has {}", a_rows * inner, a_rows, inner, va.len()));
            }
            if vb.len() != b_rows * inner {
                return Err(format!("nungw.matmul_tb: the second matrix should have {} values ({}x{}), it has {}", b_rows * inner, b_rows, inner, vb.len()));
            }
            let mut out: Vec<f64> = Vec::with_capacity(a_rows * b_rows);
            // SAFETY: matmul_trans writes all a_rows*b_rows elements before they are read.
            unsafe { out.set_len(a_rows * b_rows) };
            matmul_trans(&mut out, &va, &vb, a_rows, inner, b_rows, false, true);
            grid_val_shaped(out, vec![a_rows, b_rows])
        }
        CounterItems => count_into(seq(&args[0]).borrow().iter().map(|v| text(v).to_string())),
        CounterChars => count_into(text(&args[0]).chars().map(|c| c.to_string())),
        CounterWords => count_into(text(&args[0]).split_whitespace().map(|w| w.to_string())),
        #[cfg(not(feature = "native"))]
        WebPost | WebStatus | WebDownload => return Err(NET_OFF.to_string()),
        #[cfg(feature = "native")]
        WebPost => {
            let (url, body) = (text(&args[0]), text(&args[1]));
            let r = ureq::post(url)
                .timeout(std::time::Duration::from_secs(30))
                .send_string(body)
                .map_err(|e| web_error("web.post", url, e))?;
            string(r.into_string().map_err(|e| format!("web.post: cannot read the reply: {}", e))?)
        }
        #[cfg(feature = "native")]
        WebStatus => {
            let url = text(&args[0]);
            match ureq::get(url).timeout(std::time::Duration::from_secs(30)).call() {
                Ok(r) => Value::Int(i64::from(r.status())),
                Err(ureq::Error::Status(code, _)) => Value::Int(i64::from(code)),
                Err(ureq::Error::Transport(t)) => return Err(format!("web.status: cannot reach \"{}\": {}", url, t)),
            }
        }
        #[cfg(feature = "native")]
        WebDownload => {
            let (url, path) = (text(&args[0]), text(&args[1]));
            let r = ureq::get(url).timeout(std::time::Duration::from_secs(30)).call().map_err(|e| web_error("web.download", url, e))?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut r.into_reader(), &mut bytes).map_err(|e| format!("web.download: {}", e))?;
            std::fs::write(path, &bytes).map_err(|e| format!("web.download: cannot write \"{}\": {}", path, e))?;
            Value::Null
        }
        DataPretty => {
            let v: Json = serde_json::from_str(text(&args[0])).map_err(|e| format!("data.pretty: invalid JSON: {}", e))?;
            string(serde_json::to_string_pretty(&v).unwrap_or_default())
        }
        DataType => {
            let root = json_root(&args[0])?;
            string(match json_at(&root, text(&args[1]))? {
                Json::Null => "none",
                Json::Bool(_) => "bool",
                Json::Number(_) => "number",
                Json::String(_) => "text",
                Json::Array(_) => "list",
                Json::Object(_) => "object",
            })
        }

        other => panic!("internal error: {:?} is not a library function", other),
    })
}

/// (year, month, day, hour, minute, second) in local time.
#[cfg(windows)]
fn local_time() -> (u16, u16, u16, u16, u16, u16) {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(t: *mut SystemTime);
    }
    let mut t = SystemTime { year: 0, month: 0, day_of_week: 0, day: 0, hour: 0, minute: 0, second: 0, milliseconds: 0 };
    // SAFETY: GetLocalTime fills the struct it is given and keeps no pointer.
    unsafe { GetLocalTime(&mut t) };
    (t.year, t.month, t.day, t.hour, t.minute, t.second)
}

/// (year, month, day, hour, minute, second) in UTC.
#[cfg(not(windows))]
fn local_time() -> (u16, u16, u16, u16, u16, u16) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year as u16, month as u16, day as u16, (rem / 3600) as u16, (rem % 3600 / 60) as u16, (rem % 60) as u16)
}

#[cfg(test)]
mod gpu_transpile_tests {
    use super::*;

    #[test]
    fn transpiles_a_fused_kernel_to_wgsl() {
        // sqrt(in0^2 + in1^2)
        let prog = parse_program("l0 sq l1 sq + q").unwrap();
        let wgsl = program_to_wgsl(&prog, &[], 10).unwrap();
        assert!(wgsl.contains("@group(0) @binding(0) var<storage, read> in0: array<f32>;"));
        assert!(wgsl.contains("@group(0) @binding(1) var<storage, read> in1: array<f32>;"));
        assert!(wgsl.contains("@group(0) @binding(2) var<storage, read_write> outp: array<f32>;"));
        assert!(wgsl.contains("outp[i] = sqrt(((in0[i] * in0[i]) + (in1[i] * in1[i])));"));
        assert!(wgsl.contains("if (i >= 10u)"));
        assert!(wgsl.contains("@compute @workgroup_size(256)"));
    }

    #[test]
    fn bakes_scalars_as_literals() {
        let prog = parse_program("l0 s0 *").unwrap();
        let wgsl = program_to_wgsl(&prog, &[2.0], 4).unwrap();
        assert!(wgsl.contains("outp[i] = (in0[i] * f32(2.0));"));
    }

    #[test]
    fn rejects_a_non_finite_scalar() {
        let prog = parse_program("l0 s0 +").unwrap();
        assert!(program_to_wgsl(&prog, &[f64::INFINITY], 4).is_none());
    }
}
