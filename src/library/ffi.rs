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
