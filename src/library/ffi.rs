//! The "ffi" module: load a native shared library (.dll/.so/.dylib) and call its
//! C functions by name. This runs arbitrary native code, so every builtin is
//! gated by `--allow-run`. It supports up to 6 scalar arguments, all 64-bit
//! integers (`ffi.int`) or all 64-bit floats (`ffi.real`); strings, pointers,
//! structs and callbacks are not handled yet.
use std::cell::RefCell;

thread_local! {
    static LIBS: RefCell<Vec<libloading::Library>> = const { RefCell::new(Vec::new()) };
    // C strings handed to native code, kept alive for the session so the pointers
    // returned by ffi.cstr never dangle.
    static CSTRS: RefCell<Vec<std::ffi::CString>> = const { RefCell::new(Vec::new()) };
}

/// Copies a Globw string into a null-terminated C string and returns its address
/// (as an integer pointer) to pass to `ffi.int` as a `char*`. The buffer lives for
/// the rest of the program.
pub(super) fn ffi_cstr(s: &str) -> Result<i64, String> {
    let c = std::ffi::CString::new(s).map_err(|_| "ffi.cstr: the string contains a null byte".to_string())?;
    let ptr = c.as_ptr() as i64;
    CSTRS.with(|v| v.borrow_mut().push(c));
    Ok(ptr)
}

/// Reads a null-terminated C string at the given address into a Globw string.
pub(super) fn ffi_read(ptr: i64) -> Result<String, String> {
    if ptr == 0 {
        return Err("ffi.read: the pointer is null".to_string());
    }
    // SAFETY: trusts the pointer to be a valid, null-terminated C string; this is
    // the unavoidable trust of an FFI, which is why the module needs --allow-run.
    unsafe { Ok(std::ffi::CStr::from_ptr(ptr as *const std::os::raw::c_char).to_string_lossy().into_owned()) }
}

/// Loads a shared library by path and returns a handle to it.
pub(super) fn ffi_open(path: &str) -> Result<i64, String> {
    // SAFETY: loading a user-named shared library is inherently unsafe; it is
    // reached only with the `run` capability (--allow-run).
    let lib = unsafe { libloading::Library::new(path) }.map_err(|e| format!("ffi.open: cannot load {}: {}", path, e))?;
    Ok(LIBS.with(|l| {
        let mut v = l.borrow_mut();
        v.push(lib);
        (v.len() - 1) as i64
    }))
}

/// Calls `name` in library `h` with integer arguments, returning an integer.
pub(super) fn ffi_call_int(h: i64, name: &str, args: &[i64]) -> Result<i64, String> {
    LIBS.with(|l| {
        let v = l.borrow();
        let lib = usize::try_from(h).ok().and_then(|i| v.get(i)).ok_or("ffi: invalid library handle")?;
        let sym = name.as_bytes();
        let nf = |e: libloading::Error| format!("ffi: no function \"{}\" ({})", name, e);
        // SAFETY: calling into native code; the C signature is assumed to match
        // the declared integer arity (this is what an FFI inherently trusts).
        unsafe {
            Ok(match args.len() {
                0 => lib.get::<unsafe extern "C" fn() -> i64>(sym).map_err(nf)?(),
                1 => lib.get::<unsafe extern "C" fn(i64) -> i64>(sym).map_err(nf)?(args[0]),
                2 => lib.get::<unsafe extern "C" fn(i64, i64) -> i64>(sym).map_err(nf)?(args[0], args[1]),
                3 => lib.get::<unsafe extern "C" fn(i64, i64, i64) -> i64>(sym).map_err(nf)?(args[0], args[1], args[2]),
                4 => lib.get::<unsafe extern "C" fn(i64, i64, i64, i64) -> i64>(sym).map_err(nf)?(args[0], args[1], args[2], args[3]),
                5 => lib.get::<unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i64>(sym).map_err(nf)?(args[0], args[1], args[2], args[3], args[4]),
                6 => lib.get::<unsafe extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(sym).map_err(nf)?(args[0], args[1], args[2], args[3], args[4], args[5]),
                _ => return Err("ffi.int: at most 6 arguments are supported".to_string()),
            })
        }
    })
}

/// Like `ffi_call_int`, but for a C function that returns a 32-bit `int` (sign
/// extended to Globw's 64-bit integer). Arguments are still passed in 64-bit
/// registers, which is correct for `int` parameters on the common ABIs.
pub(super) fn ffi_call_int32(h: i64, name: &str, args: &[i64]) -> Result<i64, String> {
    LIBS.with(|l| {
        let v = l.borrow();
        let lib = usize::try_from(h).ok().and_then(|i| v.get(i)).ok_or("ffi: invalid library handle")?;
        let sym = name.as_bytes();
        let nf = |e: libloading::Error| format!("ffi: no function \"{}\" ({})", name, e);
        // SAFETY: as ffi_call_int, for a C `int` (i32) return.
        unsafe {
            Ok(i64::from(match args.len() {
                0 => lib.get::<unsafe extern "C" fn() -> i32>(sym).map_err(nf)?(),
                1 => lib.get::<unsafe extern "C" fn(i64) -> i32>(sym).map_err(nf)?(args[0]),
                2 => lib.get::<unsafe extern "C" fn(i64, i64) -> i32>(sym).map_err(nf)?(args[0], args[1]),
                3 => lib.get::<unsafe extern "C" fn(i64, i64, i64) -> i32>(sym).map_err(nf)?(args[0], args[1], args[2]),
                4 => lib.get::<unsafe extern "C" fn(i64, i64, i64, i64) -> i32>(sym).map_err(nf)?(args[0], args[1], args[2], args[3]),
                5 => lib.get::<unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i32>(sym).map_err(nf)?(args[0], args[1], args[2], args[3], args[4]),
                6 => lib.get::<unsafe extern "C" fn(i64, i64, i64, i64, i64, i64) -> i32>(sym).map_err(nf)?(args[0], args[1], args[2], args[3], args[4], args[5]),
                _ => return Err("ffi.int32: at most 6 arguments are supported".to_string()),
            }))
        }
    })
}

/// Calls `name` in library `h` with floating-point arguments, returning a float.
pub(super) fn ffi_call_real(h: i64, name: &str, args: &[f64]) -> Result<f64, String> {
    LIBS.with(|l| {
        let v = l.borrow();
        let lib = usize::try_from(h).ok().and_then(|i| v.get(i)).ok_or("ffi: invalid library handle")?;
        let sym = name.as_bytes();
        let nf = |e: libloading::Error| format!("ffi: no function \"{}\" ({})", name, e);
        // SAFETY: as ffi_call_int, for a double-precision C signature.
        unsafe {
            Ok(match args.len() {
                0 => lib.get::<unsafe extern "C" fn() -> f64>(sym).map_err(nf)?(),
                1 => lib.get::<unsafe extern "C" fn(f64) -> f64>(sym).map_err(nf)?(args[0]),
                2 => lib.get::<unsafe extern "C" fn(f64, f64) -> f64>(sym).map_err(nf)?(args[0], args[1]),
                3 => lib.get::<unsafe extern "C" fn(f64, f64, f64) -> f64>(sym).map_err(nf)?(args[0], args[1], args[2]),
                4 => lib.get::<unsafe extern "C" fn(f64, f64, f64, f64) -> f64>(sym).map_err(nf)?(args[0], args[1], args[2], args[3]),
                5 => lib.get::<unsafe extern "C" fn(f64, f64, f64, f64, f64) -> f64>(sym).map_err(nf)?(args[0], args[1], args[2], args[3], args[4]),
                6 => lib.get::<unsafe extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64>(sym).map_err(nf)?(args[0], args[1], args[2], args[3], args[4], args[5]),
                _ => return Err("ffi.real: at most 6 arguments are supported".to_string()),
            })
        }
    })
}

// ---- raw memory, for building C structs and reading out-parameters ----
// Most C APIs take a *pointer* to a struct, so you allocate a buffer, write the
// fields at their byte offsets, pass the pointer (as an integer) to ffi.int, then
// read fields back. Every address here is trusted, so the module needs --allow-run.

thread_local! {
    // buffers handed to native code, kept alive for the session.
    static BUFS: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
}

/// Allocates `n` zeroed bytes and returns their address. The buffer lives for the
/// rest of the program (a simple arena; there is no free).
pub(super) fn ffi_alloc(n: i64) -> Result<i64, String> {
    if !(0..=(1 << 30)).contains(&n) {
        return Err("ffi.alloc: the size must be between 0 and 1 GiB".to_string());
    }
    let buf = vec![0u8; n as usize];
    let ptr = buf.as_ptr() as i64; // the heap data does not move when the Vec is stored
    BUFS.with(|v| v.borrow_mut().push(buf));
    Ok(ptr)
}

/// Writes a 64-bit integer at `ptr + off`.
pub(super) fn ffi_poke_int(ptr: i64, off: i64, v: i64) {
    // SAFETY: writes to a user-provided address; this is raw memory access, gated
    // by --allow-run.
    unsafe { ((ptr + off) as *mut i64).write_unaligned(v) }
}
/// Writes a 32-bit integer (a C `int`) at `ptr + off`.
pub(super) fn ffi_poke_i32(ptr: i64, off: i64, v: i64) {
    unsafe { ((ptr + off) as *mut i32).write_unaligned(v as i32) }
}
/// Writes a double at `ptr + off`.
pub(super) fn ffi_poke_real(ptr: i64, off: i64, v: f64) {
    unsafe { ((ptr + off) as *mut f64).write_unaligned(v) }
}
/// Reads a 64-bit integer at `ptr + off`.
pub(super) fn ffi_peek_int(ptr: i64, off: i64) -> i64 {
    unsafe { ((ptr + off) as *const i64).read_unaligned() }
}
/// Reads a 32-bit integer (a C `int`) at `ptr + off`, sign extended.
pub(super) fn ffi_peek_i32(ptr: i64, off: i64) -> i64 {
    unsafe { i64::from(((ptr + off) as *const i32).read_unaligned()) }
}
/// Reads a double at `ptr + off`.
pub(super) fn ffi_peek_real(ptr: i64, off: i64) -> f64 {
    unsafe { ((ptr + off) as *const f64).read_unaligned() }
}

// ---- optional: a fully general call via libffi (feature "ffi-full") ----
// Handles any C signature, including a mix of integer and floating-point
// arguments that the hand-rolled paths above cannot. Compiled only with
// --features ffi-full (which builds the C libffi).

/// The result of a general call: an integer or a double.
#[cfg(feature = "ffi-full")]
pub(super) enum FfiRet {
    Int(i64),
    Real(f64),
}

/// Calls `name` in library `h` with the signature `sig` (one character per
/// argument: `i` int64, `d` double, `f` float32, `p` pointer), its arguments
/// given as doubles (an `i`/`p` slot is rounded/cast to an integer/pointer). The
/// return is an integer (`ret_int`) or a double.
#[cfg(feature = "ffi-full")]
pub(super) fn ffi_call_sig(h: i64, name: &str, sig: &str, args: &[f64], ret_int: bool) -> Result<FfiRet, String> {
    use libffi::middle::{arg, Cif, CodePtr, Type};
    use std::os::raw::c_void;
    let n = sig.chars().count();
    if n != args.len() {
        return Err(format!("ffi.call: signature \"{}\" expects {} arguments, got {}", sig, n, args.len()));
    }
    enum Slot {
        I(i64),
        D(f64),
        F(f32),
        P(*const c_void),
    }
    let mut types: Vec<Type> = Vec::with_capacity(n);
    let mut slots: Vec<Slot> = Vec::with_capacity(n);
    for (c, &x) in sig.chars().zip(args) {
        match c {
            'i' => {
                types.push(Type::i64());
                slots.push(Slot::I(x as i64));
            }
            'd' => {
                types.push(Type::f64());
                slots.push(Slot::D(x));
            }
            'f' => {
                types.push(Type::f32());
                slots.push(Slot::F(x as f32));
            }
            'p' => {
                types.push(Type::pointer());
                slots.push(Slot::P(x as i64 as usize as *const c_void));
            }
            other => return Err(format!("ffi.call: unknown type '{}' in signature \"{}\"", other, sig)),
        }
    }
    let call_args: Vec<_> = slots
        .iter()
        .map(|s| match s {
            Slot::I(v) => arg(v),
            Slot::D(v) => arg(v),
            Slot::F(v) => arg(v),
            Slot::P(v) => arg(v),
        })
        .collect();
    let cif = Cif::new(types, if ret_int { Type::i64() } else { Type::f64() });
    LIBS.with(|l| {
        let vlibs = l.borrow();
        let lib = usize::try_from(h).ok().and_then(|i| vlibs.get(i)).ok_or("ffi: invalid library handle")?;
        // SAFETY: resolving and calling an arbitrary C symbol with the declared
        // signature; this is the trust an FFI inherently places in the caller.
        unsafe {
            let s = lib
                .get::<unsafe extern "C" fn()>(name.as_bytes())
                .map_err(|e| format!("ffi: no function \"{}\" ({})", name, e))?;
            let code = CodePtr(*s as *mut c_void);
            Ok(if ret_int {
                FfiRet::Int(cif.call::<i64>(code, &call_args))
            } else {
                FfiRet::Real(cif.call::<f64>(code, &call_args))
            })
        }
    })
}
