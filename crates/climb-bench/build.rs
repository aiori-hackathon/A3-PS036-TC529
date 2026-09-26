//! Build script for `climb-bench`.
//!
//! `climb-mtl-sys` (a direct dependency) publishes its resolved native-library
//! directory via `cargo::metadata` (because it declares `links = "mtlslib"`),
//! which Cargo exposes here as `DEP_MTLSLIB_LIB_DIR`. We emit an rpath so the
//! benchmark binary can find `liboqs.so.9` and `libcrypto.so` at runtime without
//! a system-wide install or `LD_LIBRARY_PATH`.

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let lib_dir = env::var("DEP_MTLSLIB_LIB_DIR").unwrap_or_else(|_| {
        panic!(
            "climb-bench: DEP_MTLSLIB_LIB_DIR was not provided by climb-mtl-sys. \
             Verify that climb-mtl-sys still sets `links = \"mtlslib\"` and prints \
             `cargo::metadata=lib_dir=...`."
        )
    });

    println!("cargo:rustc-link-arg=-Wl,-rpath,{lib_dir}");
}
