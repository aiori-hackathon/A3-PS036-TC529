//! Build script for `xtask`.
//!
//! `climb-mtl-sys` (a direct dependency) publishes its resolved native-library
//! directory via `cargo::metadata` (because it declares `links = "mtlslib"`),
//! which Cargo exposes here as `DEP_MTLSLIB_LIB_DIR`. We emit an rpath so the
//! `xtask` binary can find `liboqs.so.9` and `libcrypto.so` at runtime without
//! `LD_LIBRARY_PATH`.
//!
//! The rpath baked into `climb-mtl-sys` does not propagate to dependent crates
//! — this is the Phase 3 finding: each binary that calls into the native
//! libraries at runtime needs its own rpath.  `xtask` needs it because
//! `wire-report` calls `climb_bench::measure_wire_sizes`, which ultimately
//! invokes libMTL's native signing code.

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let lib_dir = env::var("DEP_MTLSLIB_LIB_DIR").unwrap_or_else(|_| {
        panic!(
            "xtask: DEP_MTLSLIB_LIB_DIR was not provided by climb-mtl-sys. \
             Verify that climb-mtl-sys still sets `links = \"mtlslib\"` and prints \
             `cargo::metadata=lib_dir=...`."
        )
    });

    println!("cargo:rustc-link-arg=-Wl,-rpath,{lib_dir}");
}
