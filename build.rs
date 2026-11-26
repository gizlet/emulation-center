//! With `gizlet-os`, raylib-sys links a raylib built elsewhere: Gizlet
//! runtime 1's libraylib.so.600, in the directory GIZLET_RAYLIB_LIB names.
//! Its own libraries (EGL, GLES, GBM, DRM) exist only on the device.

fn main() {
    println!("cargo:rerun-if-env-changed=GIZLET_RAYLIB_LIB");
    if std::env::var_os("CARGO_FEATURE_GIZLET_OS").is_none() {
        return;
    }
    if let Some(dir) = std::env::var_os("GIZLET_RAYLIB_LIB") {
        println!("cargo:rustc-link-search=native={}", dir.to_string_lossy());
    }
    println!("cargo:rustc-link-arg-bins=-Wl,--allow-shlib-undefined");
}
