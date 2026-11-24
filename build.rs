//! On Windows the GPU backend imports graphics DLLs (OpenGL, DXGI, SetupAPI,
//! OLE Automation) that Windows would load at every start, costing about 60 ms
//! even to programs that never draw. They are delay-loaded instead: each one is
//! loaded the first time the GPU is actually used.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    let gpu = std::env::var_os("CARGO_FEATURE_GPU").is_some();
    if gpu && target.contains("windows-msvc") {
        for dll in ["opengl32.dll", "dxgi.dll", "setupapi.dll", "oleaut32.dll"] {
            println!("cargo:rustc-link-arg-bin=gw=/DELAYLOAD:{}", dll);
        }
        println!("cargo:rustc-link-arg-bin=gw=delayimp.lib");
    }
}
